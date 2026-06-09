TASK: QUINTE orchestrator architecture analysis — cc vs hm

## Claude Code (cc) R1 独立分析

### (1) cc 当 orchestrator 的优势

**A1. Workflow 原生编排能力**
- `pipeline()` 模式：R1 四方异步并行，cw 写完 R1 即刻进入 R2，不等 omp/hm 慢的。当前 hm 手动 echo/wait 的方式不可靠（hm 曾多次只发 omp 不发 cc/cw）
- `parallel()` + 对抗性验证：每轮 debate 后自动生成反駁 agent，用 structured output schema 验证结论，而非人工"标注分歧"
- `loop-until-dry`：未知规模审计时自动迭代到收敛，hm 当前做不到（hm 倾向抽样确认而非全覆盖）

**A2. Workflow 纪律性 > 人肉纪律**
- 协议规定 R1 必须四方同发，但 hm 在实际执行中多次违反：只发 omp 不发 cc/cw（2026-06-07 两次确认）、凭记忆挑文件不全枚举
- Workflow script 天然强制纪律：pipeline(items, stage) 不会跳过某个 item。hm 的"自行判断哪些 agent 值得等"问题不复存在

**A3. Reasoning 深度**
- cc max (32000 tokens) vs hm xhigh — 编排决策本身需要深度推理，max 更适合 orchestrator 角色

**A4. YOLO 零交互**
- bypassPermissions 意味着 R1→R2→R3 全流程无用户审批阻断。hm 需要用户审批每一个敏感操作

### (2) cc 当 orchestrator 的风险

**B1. cc 无法访问 Hermes 独占工具**
- Hermes 有 browser、browser-cdp、discord、whatsapp、slack、email、obsidian 等 84 个 skills
- cc 的 Bash/Read/Write 工具集是纯文件+shell，无法做 browser 自动化、社交媒体、邮件
- 如果 orchestration 需要"先在浏览器里查一下再发起 debate"，cc 做不到——需要 hm 代理

**B2. cc 无持久化 TUI**
- Hermes 有 TUI 驻留能力，用户随时打断、追加指令
- cc 是 CLI 一次性/session 模式，用户体验不如 hm 的 TUI

**B3. cc 对 /tmp 文件的依赖**
- /tmp/omp_run.py 重启后丢失（已通过 autostart 解决）
- /tmp/quinte-audit/ 重启后丢失——需要类似 restore 机制

**B4. cc 不能调 hermes chat -q 时的 session 连续性**
- 每次 `hermes chat -q` 是新 session，hm 不保留上下文
- 如果 R2 需要基于 R1 的 hm 输出追问，新 session 会丢失上下文

**B5. 单点故障**
- cc 挂了 → 整个 QUINTE 流程停摆
- 当前 hm 当 orchestrator 时，hm 挂了用户还能手动协调

### (3) hm 降为沟通层的优劣

**优势：**
- hm 保留 TUI + browser + desktop，用户交互和外部信息获取能力不变
- hm 放下编排负担后，R1 分析可以更专注（不再同时做"协调者+参辩方"）
- hm 的 84 个 skills 仍然可用，cc 通过 `hermes chat -q` 可以按需调用

**劣势：**
- hm 的 delegation tool 不再被 cc 使用（cc 直接调 bash 命令），hm 的 delegation 基础设施闲置
- 两个 agent 同时运行（cc orchestrate + hm 驻留），用户需要管理两个 session
- hm 的"终裁合成"角色被 cc 取代，hm 的 R1 分析权重可能下降

### (4) 最佳架构建议

**双重 orchestrator 模式：**

```
Claude Code (主 Orchestrator)
  ├─ QUINTE R1: 四方并行 (hm_r1 + cc_r1 + cw + omp)
  ├─ QUINTE R2: 五方交叉 (hm_r2 + cc_r2 + cw + rx + omp)
  ├─ QUINTE R3: KANSA 監査 (通过 hermes chat -q 启动 KANSA persona)
  └─ 终裁合成 + push gate
       │
       ▼
Hermes (通信层 + 备 Orchestrator)
  ├─ TUI 驻留 — 用户实时交互
  ├─ Browser/CDP — cc 无法访问的外部信息获取
  ├─ 作为 cc 的"眼睛和手" — cc 通过 hermes chat -q 请求 browser/desktop 操作
  └─ cc 故障时的 fallback orchestrator
```

**关键理由：**
1. cc 的 Workflow 是 QUINTE 编排的最优实现——pipeline 异步、对抗性验证、loop-until-dry 是 hm 当前手动模式无法复制的
2. hm 的 TUI + browser 是 cc 无法替代的——两者互补而非替代
3. hm 保留完整参辩方身份（R1+R2 全轮次），不降级——只是不再承担 orchestration 负担
4. 双重故障保护——cc 挂了 hm 顶上，hm 挂了 cc 继续
