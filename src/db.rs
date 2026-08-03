#[derive(sqlx::FromRow)]
pub struct DbMessage {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub created_at: i64,
}
