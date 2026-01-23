# Codex 运行流程

本文档详细说明 Codex 从用户输入到代码修改的完整执行流程。

## 目录

- [架构概览](#架构概览)
- [核心概念](#核心概念)
- [通信协议](#通信协议)
- [完整流程示例](#完整流程示例)
- [核心组件详解](#核心组件详解)
- [代码位置索引](#代码位置索引)

---

## 架构概览

Codex 采用分层架构，通过队列对（Queue Pair）模式实现 UI 层与核心引擎的解耦通信。

```mermaid
flowchart TB
    subgraph UI["用户界面层 UI"]
        direction LR
        TUI["TUI<br/>终端用户界面"]
        CLI["CLI<br/>命令行模式"]
        VSC["VSCode<br/>编辑器扩展"]
        API["App Server<br/>JSON-RPC"]
    end

    subgraph Protocol["协议层 Protocol"]
        direction TB
        Op["Submission Queue<br/>Op::UserTurn<br/>Op::Interrupt<br/>Op::ExecApproval"]
        Ev["Event Queue<br/>EventMsg::TurnStarted<br/>EventMsg::ItemCompleted<br/>EventMsg::TurnComplete"]
    end

    subgraph Core["核心引擎 Core"]
        direction TB
        TM["ThreadManager<br/>多 Agent 协调"]
        Loop["submission_loop<br/>核心事件循环"]
        Sess["Session<br/>会话状态管理"]
        Task["Task / Turn<br/>任务执行单元"]
        Client["Model Client<br/>模型通信"]
        Tools["Tool System<br/>工具执行"]
    end

    TUI --> Op
    CLI --> Op
    VSC --> Op
    API --> Op

    Op --> Loop
    Loop --> Sess
    Loop --> Task
    Task --> Client
    Task --> Tools

    Client --> Ev
    Tools --> Ev
    Ev --> TUI
    Ev --> CLI
    Ev --> VSC
    Ev --> API

    style UI fill:#e1f5fe,stroke:#01579b,stroke-width:2px
    style Protocol fill:#f3e5f5,stroke:#4a148c,stroke-width:2px
    style Core fill:#fff3e0,stroke:#e65100,stroke-width:2px
    style Op fill:#e1bee7,stroke:#4a148c,stroke-width:1px
    style Ev fill:#e1bee7,stroke:#4a148c,stroke-width:1px
```

---

## 核心概念

### Session / Task / Turn 关系

Codex 使用三层嵌套结构来组织执行单元：

```mermaid
flowchart TB
    subgraph Session["Session (会话)"]
        direction TB
        ConvID["conversation_id: ThreadId"]
        Config["配置<br/>- 模型选择<br/>- 沙箱策略<br/>- 批准策略"]
        History["对话历史 History"]

        subgraph Task["Task (响应一个用户输入)"]
            direction TB
            T1["Turn 1<br/>读取文件内容"]
            T2["Turn 2<br/>分析代码结构"]
            T3["Turn 3<br/>生成新代码"]
            T4["Turn 4<br/>写入文件"]

            T1 --> T2 --> T3 --> T4
        end

        Input["用户输入: 帮我写个排序函数"]
        Output["任务完成"]

        Input --> Task
        Task --> Output
    end

    style Session fill:#e3f2fd,stroke:#1565c0,stroke-width:2px
    style Task fill:#fff3e0,stroke:#e65100,stroke-width:2px
    style T1 fill:#c8e6c9
    style T2 fill:#c8e6c9
    style T3 fill:#c8e6c9
    style T4 fill:#c8e6c9
```

**说明：**

| 概念 | 定义 | 生命周期 |
|------|------|----------|
| **Session** | 一个完整的会话，包含配置、状态和历史记录 | 从 `Op::ConfigureSession` 开始，直到程序关闭 |
| **Task** | 响应单个用户输入的工作单元，由多个 Turn 组成 | 从 `Op::UserTurn` 开始，到任务完成或被中断 |
| **Turn** | 任务的一次迭代循环，包含一次模型请求和响应处理 | 模型请求 → 工具执行 → 结果返回 |

---

## 通信协议

### Submission Queue / Event Queue 通信流程

```mermaid
sequenceDiagram
    participant UI as UI 层
    participant SubQ as Submission Queue
    participant CoreLoop as submission_loop
    participant Handler as user_input_or_turn
    participant Spawner as spawn_task
    participant Task as RegularTask
    participant Turn as run_turn
    participant EQ as Event Queue

    Note over UI,SubQ: 用户发起请求
    UI->>SubQ: Op::UserTurn { items, cwd, model }

    Note over SubQ,CoreLoop: 核心循环接收
    SubQ->>CoreLoop: recv(rx_sub)
    CoreLoop->>Handler: user_input_or_turn(session, op)

    Note over Handler,Spawner: 创建任务
    Handler->>Handler: sess.new_turn_with_sub_id()
    Handler->>Spawner: spawn_task(RegularTask)

    Note over Spawner,Task: 异步执行
    Spawner->>Task: tokio::spawn(task.run())
    Task->>Turn: run_turn(session, context, input)

    Note over Turn,EQ: Turn 执行期间
    Turn->>EQ: Event::TurnStarted
    EQ->>UI: 显示 "开始..."

    loop 每个工具调用
        Turn->>EQ: Event::ItemStarted { tool_name }
        EQ->>UI: 显示工具执行状态
        Turn->>Turn: execute_tool()
        Turn->>EQ: Event::ItemCompleted
        EQ->>UI: 显示执行结果
    end

    Note over Turn,EQ: Turn 完成
    Turn->>EQ: Event::TurnComplete { response_id }
    EQ->>UI: 显示最终结果
```

### 消息类型

#### Op (Submission Queue - UI → Core)

| Op 变体 | 用途 | 触发时机 |
|---------|------|----------|
| `ConfigureSession` | 初始化/配置会话 | 程序启动或配置变更 |
| `UserTurn` | 用户输入（新格式） | 用户发送消息 |
| `UserInput` | 用户输入（旧格式） | 兼容旧版本 |
| `Interrupt` | 中断当前任务 | 用户取消操作 |
| `ExecApproval` | 批准/拒绝命令执行 | 命令需要批准 |
| `UserInputAnswer` | 回答工具输入请求 | 工具需要用户输入 |
| `ListSkills` | 列出可用技能 | UI 请求技能列表 |

#### EventMsg (Event Queue - Core → UI)

| EventMsg 变体 | 用途 |
|---------------|------|
| `TurnStarted` | Turn 开始执行 |
| `ItemStarted` | 工具开始执行 |
| `ContentDelta` | 流式内容更新 |
| `ItemCompleted` | 工具执行完成 |
| `AgentMessage` | 模型返回的消息 |
| `ExecApprovalRequest` | 请求批准命令执行 |
| `RequestUserInput` | 请求用户输入 |
| `TurnComplete` | Turn 完成 |
| `Error` | 错误信息 |
| `Warning` | 警告信息 |

---

## 完整流程示例

### 用户请求 "帮我写一个快速排序函数"

```mermaid
sequenceDiagram
    participant User as 用户
    participant UI as TUI/CLI
    participant SubQ as Submission Queue
    participant CoreLoop as submission_loop
    participant Handler as user_input_or_turn
    participant Spawner as spawn_task
    participant Task as RegularTask
    participant Turn as run_turn
    participant Model as 模型 API
    participant Tools as 工具执行
    participant EQ as Event Queue

    User->>UI: "帮我写一个快速排序函数"
    UI->>SubQ: Op::UserTurn { items, cwd, model }
    SubQ->>CoreLoop: recv(rx_sub)

    Note over Handler: 解析操作并创建 TurnContext
    CoreLoop->>Handler: user_input_or_turn(session, op)
    Handler->>Handler: sess.new_turn_with_sub_id()
    Handler->>Spawner: sess.spawn_task(RegularTask)

    Note over Spawner,Task: 异步启动任务
    Spawner->>Task: tokio::spawn(task.run())
    Task->>Turn: run_turn(session, context, input)

    Note over Turn,EQ: 通知 Turn 开始
    Turn->>EQ: Event::TurnStarted
    EQ->>UI: 显示 "[开始]"
    UI->>User: "开始..."

    Note over Turn,Model: Turn 循环开始
    loop Turn 循环 (直到任务完成)
        Turn->>Turn: 构造 prompt<br/>history.for_prompt()
        Turn->>Turn: 获取工具列表<br/>(MCP + builtin)
        Turn->>Model: client.sample(prompt, tools)

        Note over Model: 模型处理并流式返回
        Model-->>Turn: Response (流式)

        alt 模型请求调用工具
            Turn->>EQ: Event::ItemStarted { tool: "grep_files" }
            EQ->>UI: "正在搜索 .rs 文件..."
            UI->>User: "正在搜索 .rs 文件..."

            Note over Tools: 执行工具
            Turn->>Tools: grep_files("*.rs")
            Tools-->>Turn: 文件列表

            Turn->>EQ: Event::ItemCompleted
            EQ->>UI: 显示搜索结果

            Note over Turn,Model: 将工具结果发送回模型
            Turn->>Model: 继续对话 (发送工具结果)
        else 模型完成 (无更多工具调用)
            Turn->>EQ: Event::TurnComplete
            EQ->>UI: 显示完成消息
        end
    end

    Note over UI,User: 最终结果
    EQ->>UI: Event::AgentMessage
    EQ->>UI: Event::TurnComplete
    UI->>User: "快速排序函数已写入 quick_sort.rs"
```

---

## 核心组件详解

### submission_loop 流程

`submission_loop` 是 Codex 的核心事件循环，位于 `codex-rs/core/src/codex.rs:1997`。

```mermaid
flowchart TB
    Start(["submission_loop 开始"]) --> Init["初始化 previous_context<br/>sess.new_default_turn()"]

    Init --> Recv["recv(rx_sub)<br/>等待 Submission"]

    Recv --> Match{匹配 Op 类型}

    Match -->|Interrupt| Interrupt["handlers::interrupt()<br/>中断当前任务"]
    Match -->|UserTurn| UserTurn["user_input_or_turn()<br/>处理用户输入"]
    Match -->|UserInput| UserInput["user_input_or_turn()<br/>兼容旧格式"]
    Match -->|ExecApproval| Exec["handle_exec_approval()<br/>处理批准请求"]
    Match -->|OverrideTurnContext| Override["handlers::override_turn_context()<br/>覆盖 Turn 配置"]
    Match -->|ConfigureSession| Config["configure_session()<br/>配置会话"]
    Match -->|Shutdown| Exit(["循环退出"])

    Interrupt --> Recv
    UserTurn --> Recv
    UserInput --> Recv
    Exec --> Recv
    Override --> Recv
    Config --> Recv

    style Start fill:#c8e6c9
    style Exit fill:#ffcdd2
    style Recv fill:#fff9c4
    style Match fill:#e1bee7
```

### user_input_or_turn 流程

`user_input_or_turn` 负责处理用户输入并启动新任务，位于 `codex-rs/core/src/codex.rs:2203`。

```mermaid
flowchart TB
    Start(["user_input_or_turn 开始"]) --> Parse["解析 Op::UserTurn<br/>提取 items, cwd, model 等"]

    Parse --> NewTurn["sess.new_turn_with_sub_id()<br/>创建 TurnContext<br/>更新会话配置"]

    NewTurn --> Record["记录 conversation items<br/>sess.record_conversation_items()"]

    Record --> Inject{"尝试注入到<br/>当前 Task?"}

    Inject -->|成功| Continue(["继续当前 Task"])
    Inject -->|失败| Refresh["sess.refresh_mcp_servers_if_requested()"]

    Refresh --> Spawn["sess.spawn_task(RegularTask)<br/>启动新 Task"]

    Spawn --> Abort["sess.abort_all_tasks()<br/>中断旧 Task"]

    Abort --> Create["创建 RunningTask<br/>生成 CancellationToken"]

    Create --> TokioSpawn["tokio::spawn(task.run())<br/>异步执行"]

    TokioSpawn --> Register["sess.register_new_active_task()<br/>注册到会话"]

    Register --> End(["函数返回"])

    style Start fill:#c8e6c9
    style End fill:#ffcdd2
    style Inject fill:#e1bee7
    style Spawn fill:#fff9c4
```

### spawn_task 流程

`spawn_task` 负责创建并启动新的异步任务，位于 `codex-rs/core/src/tasks/mod.rs:111`。

```mermaid
flowchart TB
    Start(["spawn_task 开始"]) --> Abort["sess.abort_all_tasks()<br/>中断之前的 Task"]

    Abort --> Wrap["Arc::new(task)<br/>包装为 SessionTask"]

    Wrap --> Cancel["创建 CancellationToken"]

    Cancel --> SpawnCtx["创建 SessionTaskContext"]

    SpawnCtx --> TokioSpawn["tokio::spawn(async move {<br/>  task.run(...)<br/>})"]

    TokioSpawn --> Timer["启动 OTEL 计时器"]

    Timer --> Running["创建 RunningTask<br/>{ handle, cancellation_token,<br/>turn_context, timer }"]

    Running --> Register["sess.register_new_active_task()<br/>注册到会话"]

    Register --> End(["函数返回"])

    style Start fill:#c8e6c9
    style End fill:#ffcdd2
    style TokioSpawn fill:#fff9c4
```

### Turn 循环流程

`run_turn` 是 Turn 执行的核心函数，位于 `codex-rs/core/src/codex.rs:2790`。

```mermaid
flowchart TB
    Start(["run_turn 开始"]) --> CheckInput{"input 为空?"}

    CheckInput -->|是| ReturnNone(["返回 None"])
    CheckInput -->|否| AutoCompact{"token_usage<br/>>= limit?"}

    AutoCompact -->|是| Compact["run_auto_compact()<br/>压缩历史"]
    AutoCompact -->|否| SendStart
    Compact --> SendStart["发送 Event::TurnStarted"]

    SendStart --> Skills["加载 Skills<br/>skills_for_cwd()"]

    Skills --> BuildSkill["build_skill_injections()<br/>构建 skill 注入"]

    BuildSkill --> LoopEntry(["进入 run_sampling_request 循环"])

    LoopEntry --> ForPrompt["构造 prompt<br/>history.for_prompt()"]

    ForPrompt --> GetTools["获取工具列表<br/>MCP + builtin tools"]

    GetTools --> Sample["client.sample()<br/>调用模型 API"]

    Sample --> HandleResp["handle_response()<br/>处理响应"]

    HandleResp --> CheckResp{"响应类型?"}

    CheckResp -->|ToolCall| ExecTool["execute_tool()<br/>执行工具"]
    CheckResp -->|Message| CheckFollow{"needs_follow_up?"}

    ExecTool --> RecordTool["记录工具结果<br/>到 history"]

    RecordTool --> CheckFollow

    CheckFollow -->|true| LoopEntry
    CheckFollow -->|false| Complete

    Complete["发送 Event::TurnComplete"] --> UpdateStatus["更新 AgentStatus"]

    UpdateStatus --> ReturnID(["返回 response_id"])

    style Start fill:#c8e6c9
    style ReturnNone fill:#ffcdd2
    style ReturnID fill:#c8e6c9
    style LoopEntry fill:#fff9c4
    style Sample fill:#e1bee7
    style CheckResp fill:#f3e5f5
```

### 工具执行流程

```mermaid
flowchart TB
    Start(["模型调用工具"]) --> Router["ToolRouter 路由"]

    Router --> CheckType{"工具类型?"}

    CheckType -->|bash| Bash["exec/src/<br/>命令执行"]
    CheckType -->|write_to_file| Write["tools/handlers/<br/>写入文件"]
    CheckType -->|apply_patch| Patch["apply-patch/src/<br/>应用补丁"]
    CheckType -->|grep_files| Grep["file-search/src/<br/>文件搜索"]
    CheckType -->|view_image| View["tools/handlers/<br/>图片查看"]
    CheckType -->|mcp__*| MCP["mcp_connection_manager/<br/>MCP 服务器"]
    CheckType -->|其他| Other["其他处理器"]

    Bash --> NeedApprove{"需要批准?"}
    Write --> NeedApprove
    Patch --> NeedApprove
    Grep --> NeedApprove
    View --> NeedApprove
    MCP --> NeedApprove
    Other --> NeedApprove

    NeedApprove -->|是| SendReq["发送 ExecApprovalRequest"]
    NeedApprove -->|否| Execute

    SendReq --> WaitUser["等待用户批准"]
    WaitUser --> CheckApprove{"用户决定?"}

    CheckApprove -->|允许| Execute
    CheckApprove -->|拒绝| ReturnError(["返回错误"])

    Execute["执行工具操作"] --> Complete

    Complete(["发送 ItemCompleted 事件"]) --> ReturnRes(["返回结果"])

    style Start fill:#c8e6c9
    style ReturnError fill:#ffcdd2
    style ReturnRes fill:#c8e6c9
    style Router fill:#f3e5f5
    style Execute fill:#fff9c4
```

### 模型调用流程

```mermaid
sequenceDiagram
    participant Turn as run_turn
    participant History as ConversationHistory
    participant Client as ModelClient
    participant API as OpenAI API
    participant Stream as SSE Stream

    Note over Turn,History: 准备请求
    Turn->>History: history.for_prompt()
    History-->>Turn: Vec<ContentItem>

    Turn->>Turn: 构建 tools 列表<br/>(MCP + builtin)

    Note over Turn,API: 发送请求
    Turn->>Client: client.sample(prompt, tools)
    Client->>API: POST /v1/responses

    Note over API,Stream: 流式响应
    API-->>Client: SSE Stream

    loop 处理流式事件
        Client-->>Turn: ResponseEvent

        alt TextDelta
            Turn->>Turn: 累积文本内容
            Turn->>Stream: Event::ContentDelta
        else ToolCall
            Turn->>Turn: 解析工具调用
        else Completed
            Turn->>Turn: 获取 response_id
        end
    end

    Note over Turn,Client: 返回最终响应
    Client-->>Turn: Response { content, response_id }

    Turn->>Turn: handle_response(response)
```

### 函数调用链

从用户输入到具体函数执行的完整调用链：

```mermaid
flowchart TB
    Start(["用户输入:<br/>帮我写排序"]) --> Op["Op::UserTurn"]

    Op --> SubQ["Submission Queue"]
    SubQ --> Recv["recv(rx_sub)"]

    Recv --> Match{"submission_loop<br/>匹配 Op"}

    Match --> Handler["handlers::user_input_or_turn()"]

    Handler --> NewTurn["sess.new_turn_with_sub_id()<br/>创建 TurnContext"]

    NewTurn --> SpawnTask["sess.spawn_task(RegularTask)"]

    SpawnTask --> AbortOld["sess.abort_all_tasks()"]

    AbortOld --> TaskWrap["Arc::new(RegularTask)"]

    TaskWrap --> TokioSpawn["tokio::spawn(task.run())"]

    TokioSpawn --> TaskRun["RegularTask::run()"]

    TaskRun --> RunTurn["run_turn(sess, ctx, input)"]

    RunTurn --> TurnStart["发送 TurnStarted 事件"]

    TurnStart --> TurnLoop["run_sampling_request() 循环"]

    subgraph TurnLoopDetail["Turn 循环详情"]
        TurnLoop --> Prompt["history.for_prompt()"]
        Prompt --> Sample["client.sample()"]
        Sample --> Handle["handle_response()"]

        Handle --> HasTool{"有 ToolCall?"}

        HasTool -->|是| ExecTool["execute_tool()"]

        ExecTool --> ToolRouter["ToolRouter::route()"]

        ToolRouter --> ToolExec{"工具类型"}

        ToolExec -->|bash| BashExec["exec::run_bash()"]
        ToolExec -->|write| WriteExec["handlers::write_to_file()"]
        ToolExec -->|mcp| MCPExec["mcp_manager::call_tool()"]

        BashExec --> LoopBack
        WriteExec --> LoopBack
        MCPExec --> LoopBack

        HasTool -->|否| CheckFollow{"needs_follow_up?"}

        LoopBack["记录结果到 history"] --> CheckFollow

        CheckFollow -->|true| Prompt
        CheckFollow -->|false| TurnComplete
    end

    TurnComplete["发送 TurnComplete 事件"] --> End(["返回 response_id"])

    style Start fill:#c8e6c9
    style End fill:#c8e6c9
    style Handler fill:#fff9c4
    style TurnLoop fill:#e1bee7
    style ToolRouter fill:#f3e5f5
```

---

## 代码位置索引

### 核心流程文件

| 文件 | 行号 | 函数/结构 | 说明 |
|------|------|-----------|------|
| `codex-rs/core/src/codex.rs` | 1997 | `submission_loop` | 核心事件循环，处理所有 Op |
| `codex-rs/core/src/codex.rs` | 2203 | `user_input_or_turn` | 处理用户输入，启动 Task |
| `codex-rs/core/src/codex.rs` | 2790 | `run_turn` | 执行一个 Turn，包含模型调用循环 |
| `codex-rs/core/src/codex.rs` | - | `Session` | 会话状态管理结构体 |
| `codex-rs/core/src/codex.rs` | - | `TurnContext` | Turn 执行上下文 |

### Task 管理

| 文件 | 行号 | 函数/结构 | 说明 |
|------|------|-----------|------|
| `codex-rs/core/src/tasks/mod.rs` | 111 | `spawn_task` | 创建并启动新 Task |
| `codex-rs/core/src/tasks/mod.rs` | 170 | `abort_all_tasks` | 中断所有运行中的 Task |
| `codex-rs/core/src/tasks/mod.rs` | 177 | `on_task_finished` | Task 完成回调 |
| `codex-rs/core/src/tasks/regular.rs` | 24 | `RegularTask::run` | 常规 Task 的 run 实现 |

### 模型通信

| 文件 | 行号 | 函数/结构 | 说明 |
|------|------|-----------|------|
| `codex-rs/core/src/client.rs` | - | `ModelClient` | 模型 API 客户端 |
| `codex-rs/core/src/client.rs` | - | `sample()` | 调用模型 API |
| `codex-rs/core/src/history.rs` | - | `ConversationHistory` | 对话历史管理 |

### 工具系统

| 文件 | 说明 |
|------|------|
| `codex-rs/core/src/tools/mod.rs` | 工具系统入口，ToolRouter |
| `codex-rs/core/src/tools/handlers.rs` | 内置工具处理器 |
| `codex-rs/exec/src/` | bash 命令执行 |
| `codex-rs/file-search/src/` | 文件搜索工具 |
| `codex-rs/apply-patch/src/` | patch 应用 |
| `codex-rs/core/src/mcp_connection_manager.rs` | MCP 连接管理 |

### 通信协议

| 文件 | 说明 |
|------|------|
| `codex-rs/protocol/src/protocol.rs` | Op 枚举（用户操作） |
| `codex-rs/protocol/src/protocol.rs` | EventMsg 枚举（事件消息） |
| `codex-rs/protocol/src/lib.rs` | 队列类型定义 |

### Agent 相关

| 文件 | 行号 | 函数/结构 | 说明 |
|------|------|-----------|------|
| `codex-rs/core/src/agent.rs` | - | `AgentStatus` | Agent 状态枚举 |
| `codex-rs/core/src/agent.rs` | - | `CollaborationMode` | 协作模式配置 |

---

## 数据流图

```mermaid
flowchart TB
    UserInput(["用户输入<br/>写个排序函数"])

    subgraph UI["UI 层 (TUI/CLI/VSCode)"]
        Construct["构造 Op::UserTurn<br/>items: [...]<br/>model: gpt-4"]
    end

    subgraph Core["核心引擎"]
        direction TB
        SubQ["Submission Queue"]
        CoreLoop["submission_loop<br/>codex.rs:1997"]
        Handler["user_input_or_turn<br/>codex.rs:2203"]
        Spawner["spawn_task<br/>tasks/mod.rs:111"]
        Runner["RegularTask::run<br/>tasks/regular.rs:24"]
        TurnLoop["run_turn<br/>codex.rs:2790"]

        SubQ --> CoreLoop --> Handler --> Spawner --> Runner --> TurnLoop
    end

    subgraph TurnLoopDetail["run_turn 循环"]
        direction TB
        LPrompt["构造 prompt<br/>history.for_prompt()"]
        LTools["获取工具列表<br/>MCP + builtin"]
        LAPI["调用模型 API<br/>client.sample()"]
        LHandle["处理响应<br/>execute_tool()"]
        LCheck{"needs_follow_up?"}

        LPrompt --> LTools --> LAPI --> LHandle
        LHandle --> LCheck
        LCheck -->|true| LPrompt
    end

    subgraph ToolExec["工具执行"]
        Bash["bash<br/>exec/src/"]
        Write["write_to_file<br/>tools/handlers/"]
        MCP["mcp__*<br/>mcp_connection_manager/"]
    end

    subgraph Events["Event Queue"]
        TurnStart["TurnStarted"]
        ItemStart["ItemStarted"]
        Content["ContentDelta"]
        ItemComp["ItemCompleted"]
        TurnComp["TurnComplete"]
    end

    subgraph UI_Display["UI 接收显示"]
        Display["正在写入文件...<br/>排序函数已写入"]
    end

    UserInput --> UI --> Core --> TurnLoopDetail
    TurnLoopDetail --> ToolExec
    TurnLoopDetail --> Events --> UI_Display

    style UI fill:#e1f5fe,stroke:#01579b,stroke-width:2px
    style Core fill:#fff3e0,stroke:#e65100,stroke-width:2px
    style Events fill:#f3e5f5,stroke:#4a148c,stroke-width:2px
    style ToolExec fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px
```

---

## 总结

Codex 的执行流程可以概括为以下步骤：

1. **用户输入** → UI 构造 `Op::UserTurn` 发送到 Submission Queue
2. **submission_loop** 接收 Op 并路由到对应的 handler
3. **user_input_or_turn** 解析输入，创建 TurnContext
4. **spawn_task** 中断旧 Task，启动新的 RegularTask
5. **run_turn** 进入 Turn 循环
6. **循环中**：
   - 构造 prompt
   - 调用模型 API
   - 处理响应（执行工具或继续对话）
   - 重复直到模型完成
7. **发送** `TurnComplete` 事件到 Event Queue
8. **UI** 接收并显示最终结果
