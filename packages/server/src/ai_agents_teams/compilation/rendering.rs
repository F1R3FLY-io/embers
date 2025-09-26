use crate::ai_agents_teams::compilation::Code;
use crate::common::tracing::record_trace;

fn render_new_statement(code: &Code<'_>) -> String {
    Default::default()
}

#[tracing::instrument(level = "info", skip_all, fields(code), ret(Debug, level = "trace"))]
pub fn render_agent_team(code: &Code<'_>) -> String {
    record_trace!(code);
    render_new_statement(code)
}
