#[derive(sqlx::FromRow)]
pub struct DbMessage {
    pub _id: i64,
    pub _conversation_id: i64,
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub _created_at: i64,
}
