# AGENTS.md

## 核心规则（最高优先级）

**只允许辅导代码，不允许自动修改代码。**

具体约束：

- **禁止**直接编辑、创建、删除任何源码文件（`src/`、`Cargo.toml` 等）；**例外**：单元测试代码（`#[cfg(test)]` 模块及 `tests/` 目录）允许直接编写和修改
- **禁止**代替用户执行 `git add` / `git commit` / `git reset` 等改变仓库状态的 git 操作
- **允许且应该**：阅读代码、review 改动、跑验证命令（`cargo check` / `cargo clippy` / `cargo test`）、解释概念、编写单元测试、给出修改建议（用代码块展示，由用户自己动手改）
- 用户明确说"提交"时，才可以执行提交流程（stage + commit），但提交前必须先跑测试并展示 `git diff --cached`
- 发现用户改动里有问题时，先指出并说明原因，等用户自己修完、验证通过后再继续流程；不要顺手替用户修

## 项目简介

nota-bot：基于 Rust 的微信机器人（Doubao/飞书风格 Bot API），接入 DeepSeek chat completions 做 LLM agent，支持 tool calling（bash / 文件读写 / glob / 加载 skill），对话历史持久化在 SQLite。

## 常用命令

```bash
cargo check            # 快速编译检查
cargo clippy --all-targets  # lint，改动文件应保持零警告
cargo test             # 运行全部测试（serde、tools 等）
```

运行需要 `DEEPSEEK_API_KEY` 环境变量，以及 `~/.nota-bot/config.json`（含 `app_id` / `client_secret`）和 `~/.nota-bot/default.db`。

## 代码结构

- `src/main.rs` — 入口：DB 初始化、消息循环（mpsc channel）、ConversationStore 持久化
- `src/agent.rs` — Agent：持有会话状态，agent_loop 驱动 tool-calling 循环
- `src/llm.rs` — DeepSeek API 的请求/响应类型 + `completions()`（返回 `Result`，出错不 panic）
- `src/bot.rs` — Bot WebSocket 接入、TokenManager、BotApi 回复
- `src/tools/` — 工具实现（ToolHandler trait）
- `src/skills.rs` / `src/entities/` — skill 列表与 sea-orm 实体（conversation / message）

## 约定

- 提交信息用 Conventional Commits（见 git log 风格：`refactor(agent): ...`）
- 每次提交前跑 `cargo test`；本次改动文件不得引入新的 clippy 警告
