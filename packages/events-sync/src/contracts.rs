use std::fmt::Display;

pub fn rho_init_events_channels(service_id: impl Display) -> String {
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
    service_id: impl Display,
    self_id: impl Display,
    hostname: impl Display,
    port: u16,
) -> String {
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

pub fn rho_unsubscribe_from_service(service_id: impl Display, self_id: impl Display) -> String {
    format!(
        r#"
        for(@listeners <- @"{service_id}-listeners") {{
            @"{service_id}-listeners"!(listeners.delete("{self_id}"))
        }}
        "#
    )
}
