use etc::escape_string;
use uuid::Uuid;

pub fn rho_init_events_channels(service_id: &str) -> String {
    let service_id = escape_string(service_id);
    format!(
        r#"
        @"{service_id}-listeners"!({{}})|
        contract @"{service_id}-notify-listeners"(@payload) = {{
            new loop, grpcTell(`rho:io:grpcTell`) in {{
                contract loop(@listeners, @payload) = {{
                    match listeners {{
                        [] => Nil
                        [head ...tail] => {{
                            grpcTell!(head.nth(1).get("hostname"), head.nth(1).get("port"), payload)|
                            loop!(tail)
                        }}
                    }}
                }}|
                for(@listeners <<- @"{service_id}-listeners") {{
                    loop!(listeners.toList(), payload)
                }}
            }}
        }}
        "#
    )
}

pub fn rho_subscribe_to_service(
    service_id: &str,
    self_id: Uuid,
    hostname: &str,
    port: u16,
) -> String {
    let service_id = escape_string(service_id);
    let hostname = escape_string(hostname);

    format!(
        r#"
        for(@listeners <- @"{service_id}-listeners") {{
            @"{service_id}-listeners"!(listeners.set("{self_id}", {{
                "hostname": "{hostname}",
                "port": {port},
            }}))
        }}
        "#
    )
}

pub fn rho_unsubscribe_from_service(service_id: &str, self_id: Uuid) -> String {
    let service_id = escape_string(service_id);

    format!(
        r#"
        for(@listeners <- @"{service_id}-listeners") {{
            @"{service_id}-listeners"!(listeners.delete("{self_id}"))
        }}
        "#
    )
}
