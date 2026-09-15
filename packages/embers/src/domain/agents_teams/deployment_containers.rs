//! Code placement is independent of workflow state ownership.
use std::collections::BTreeMap;

use anyhow::{Context, ensure};
use firefly_client::models::Uri;
use firefly_client::rendering::Render;

use super::compilation::AgentCodes;
use super::compilation::workflow_rendering::{render_activity_dispatch, system_channels};
use super::models::PreparedContainer;
use super::workflow::{NodeKind, WorkflowDefinition};
use crate::domain::common::{RegistryDeploy, prepare_for_signing};

#[derive(Debug, Render)]
#[template(path = "agents_teams/deploy_container.rho", blocks = ["name"])]
struct ContainerTemplate {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
    container_id: String,
    definition_hash: String,
    #[template(direct)]
    system_channels: Vec<&'static str>,
    #[template(direct)]
    dispatch: String,
}

pub fn prepare(
    definition: &WorkflowDefinition,
    claims: BTreeMap<String, RegistryDeploy>,
    agents: &AgentCodes,
    valid_after: u64,
) -> anyhow::Result<(Vec<PreparedContainer>, BTreeMap<String, String>)> {
    let containers = &definition.parsed().deployment_containers;
    ensure!(
        claims.keys().eq(containers.keys()),
        "Provide one registry claim for each deployment container."
    );
    let container_uris: BTreeMap<_, Uri> = claims
        .iter()
        .map(|(id, claim)| (id.clone(), claim.uri_pub_key.into()))
        .collect();
    let mut prepared = Vec::new();
    let mut bindings = BTreeMap::new();
    let mut uris = std::collections::BTreeSet::new();
    for (id, claim) in claims {
        let container = &containers[&id];
        let uri: Uri = claim.uri_pub_key.into();
        ensure!(
            uris.insert(uri.as_ref().clone()),
            "Deployment containers must use distinct registry URIs."
        );
        let mut members = definition.parsed().clone();
        members.nodes.retain(|node_id, node| {
            container.node_ids.contains(node_id) && matches!(node.kind, NodeKind::Activity { .. })
        });

        for node_id in members.nodes.keys() {
            ensure!(
                bindings
                    .insert(node_id.clone(), uri.as_ref().clone())
                    .is_none(),
                "Activity belongs to more than one deployment container."
            );
        }
        // An outer container dispatches its nested members through their own
        // registered contracts; it never silently flattens their code placement.
        let mut child_bindings = BTreeMap::new();
        for (child_id, child) in containers {
            let mut ancestor = child.parent_id.as_deref();
            let mut descendant = false;
            while let Some(parent_id) = ancestor {
                if parent_id == id {
                    descendant = true;
                    break;
                }
                ancestor = containers
                    .get(parent_id)
                    .and_then(|parent| parent.parent_id.as_deref())
                    .or_else(|| {
                        containers
                            .values()
                            .find(|parent| {
                                parent.node_ids.iter().any(|node_id| node_id == parent_id)
                            })
                            .map(|parent| parent.id.as_str())
                    });
            }
            if descendant {
                for node_id in &child.node_ids {
                    if let Some(node) = definition
                        .parsed()
                        .nodes
                        .get(node_id)
                        .filter(|node| matches!(node.kind, NodeKind::Activity { .. }))
                    {
                        members.nodes.insert(node_id.clone(), node.clone());
                        child_bindings
                            .insert(node_id.clone(), container_uris[child_id].as_ref().clone());
                    }
                }
            }
        }
        ensure!(
            !members.nodes.is_empty(),
            "Deployment container {} has no executable activities, directly or in a nested container.",
            container.label
        );
        let code = ContainerTemplate {
            env_uri: uri.clone(),
            version: claim.version,
            public_key: claim.uri_pub_key.serialize_uncompressed().to_vec(),
            sig: claim.signature,
            container_id: id.clone(),
            definition_hash: definition.definition_hash(),
            system_channels: system_channels(&members),
            dispatch: render_activity_dispatch(&members, agents, &child_bindings)?,
        }
        .render()
        .context("Could not compile deployment container")?;
        prepared.push(PreparedContainer {
            id,
            uri,
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .timestamp(claim.timestamp)
                .call(),
        });
    }
    prepared.sort_by_key(|prepared| {
        let mut depth = 0usize;
        let mut parent = containers[&prepared.id].parent_id.as_deref();
        while let Some(id) = parent {
            depth += 1;
            parent = containers
                .get(id)
                .and_then(|container| container.parent_id.as_deref())
                .or_else(|| {
                    containers
                        .values()
                        .find(|container| container.node_ids.iter().any(|node_id| node_id == id))
                        .map(|container| container.id.as_str())
                });
        }
        std::cmp::Reverse(depth)
    });
    Ok((prepared, bindings))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn nested_definition() -> WorkflowDefinition {
        WorkflowDefinition::from_json(json!({
            "schemaVersion": "2.0", "entryScopeId": "root",
            "nodes": {
                "input": {"id":"input","label":"Input","scopeId":"root","kind":"input"},
                "local": {"id":"local","label":"Local","scopeId":"root","kind":"activity","activity":{"kind":"builtin","name":"compress"}},
                "output": {"id":"output","label":"Output","scopeId":"root","kind":"output"}
            },
            "scopes":{"root":{"id":"root","label":"Root","kind":"root","initialNodeIds":["input"],"entryActivities":[],"exitActivities":[],"history":"none"}},
            "transitions":{}, "routeGroups":{},
            "deploymentContainers": {
                "outer":{"id":"outer","label":"Outer","nodeIds":[],"target":"current"},
                "inner":{"id":"inner","label":"Inner","nodeIds":["local"],"target":"current","parentId":"outer"}
            },
            "interface":{"inputs":{},"outputs":{}},"types":{},"policies":{"defaultExecution":"bounded","visibility":"private"},"presentation":{"nodes":{},"edges":{}}
        }).to_string()).unwrap()
    }

    #[test]
    fn nested_containers_are_distinct_contracts_and_children_finalize_first() {
        let definition = nested_definition();
        let claims = ["outer", "inner"]
            .into_iter()
            .enumerate()
            .map(|(i, id)| {
                let secret = secp256k1::SecretKey::from_byte_array([i as u8 + 1; 32]).unwrap();
                (
                    id.to_owned(),
                    RegistryDeploy {
                        timestamp: chrono::DateTime::from_timestamp_millis(1000 + i as i64)
                            .unwrap(),
                        version: 0,
                        uri_pub_key: secret.public_key(&secp256k1::Secp256k1::new()),
                        signature: vec![],
                    },
                )
            })
            .collect();
        let (prepared, bindings) = prepare(&definition, claims, &AgentCodes::new(), 0).unwrap();
        assert_eq!(
            prepared.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["inner", "outer"]
        );
        assert_ne!(prepared[0].uri, prepared[1].uri);
        assert_eq!(bindings["local"], *prepared[0].uri.as_ref());
        assert_ne!(prepared[0].contract.0, prepared[1].contract.0);
    }
}
