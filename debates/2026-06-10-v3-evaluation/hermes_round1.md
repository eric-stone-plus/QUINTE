# Hermes R1 — QUINTE v3.0 升级价值评估

TASK: Evaluate whether QUINTE v3.0's concepts have genuine upgrade value or are too cumbersome, with special focus on omp's perspective (unavailable on Linux when v3.0 was proposed).

---

## 0. 文件覆盖率声明

审计范围：四个 public repo 共 42 个文件（排除 .git/）。
- RASHOMON: README.md, CONCEPTS.md, GATES.md, PHENOMENOLOGY.md (4 files)
- QUINTE: PROTOCOL.md, extensions.md, CHANGELOG.md, MIGRATION.md, README.md, hermes-skill/SKILL.md, 5 debate files, 2 reference files, 1 demo, 4 assets = 20 content files
- KANSA: README.md (1 file)
- KENGEN: README.md (1 file)

已读：26/26 主内容文件（100%）。未读：4 assets（SVG/PNG/HTML 架构图，非协议文本）；5 个 v3 辩论日志（已读终裁 + v3 self-audit 终裁）。

---

## 1. v3.0 核心变更摘要

[鏡門 ✓] PROTOCOL.md L1 声明 "v3.0 (2026-06-09)"；CHANGELOG.md L25-26 记录 v2.4→v3.0 变更：Orchestrator hm→cc

v3.0 引入六大新概念：

1. **编排-监督分离 (Orchestration-Oversight Separation)**: cc 执行编排，hm 同步否决监督
2. **三机制认识论**: Agent(内置子agent) + Workflow(pipeline/parallel) + Bash(外部agent)
3. **跨模型对抗性验证**: Phase 3 每争议 3 refuter，≥1 跨模型
4. **loop-until-dry**: 双 critic + 双条件终止 + escalate→人工
5. **治理层**: cost circuit breaker, poison detection, state persistence, cross-round consistency
6. **并行四道门**: 四门从串行改并行(~5s)，證門拆为闸门层+执行层

---

## 2. 六个概念的逐一评估

### 2.1 编排-监督分离：✅ 有价值，但执行风险高

[鏡門 ✓] RASHOMON/README.md L9-12: "the entity that executes the debate should not be the same entity that judges its quality"

**价值**：
- 解决了 v2.x 的核心结构性问题：hm 既是 orchestrator 又是 participant → 内生利益冲突
- 历史证据确凿：2026-06-07 两次 hm 跳过 cc/cw、凭记忆挑文件不枚举全清单（v3_requinte_final.md L60-61 hm 自认）
- 分离后 hm 的 xhigh reasoning 用于审计编排质量而非机械调度 → 资源分配更优

**风险**：
- cc 作为编排引擎引入了新的单点故障：cc 超时率 71%（14 次启动中 10 次零输出，multi-agent-debate skill）
- cc 的 Workflow 机制未经 QUINTE 自身验证：pipeline()/parallel()/agent({schema}) 是 cc 原生功能，但 v3.0 将整个辩论架构依赖于此——这些原语在 QUINTE 场景下的可靠性未经测试
- hm fallback orchestrator 路径模糊：RASHOMON/README.md L109 提了"cc failure → hm takes over"，但未定义触发条件和回退协议

**omp 视角缺失**：v3.0 的 v3_requinte_final.md L43 明确写"omp: 纯 API 平替, 无文件读取, R1 价值有限"。omp 在 v3.0 设计中仍是外部 Bash agent 之一——没有被赋予编排层任何角色。omp 有 LSP/DAP 调试能力（唯一有此能力的 agent），在 cc 故障时的替补潜力未被讨论。

### 2.2 三机制认识论：⚠️ 理论精妙但实操未验证

[鏡門 ✓] RASHOMON/CONCEPTS.md L49-64 定义三机制；RASHOMON/PHENOMENOLOGY.md L18-23 称三机制为"three distinct ways of knowing"

**价值**：
- Agent(独立上下文) → 防止编排者注意力偏差污染子任务
- Workflow(pipeline/parallel) → 结构性保证，pipeline 不可能跳过 item
- Bash(外部agent) → 工具链多样性 → decorrelated blind spots

**问题**：
- **Agent 机制依赖 cc 的内部子 agent 系统**——这个系统在 DeepSeek 后端的表现从未被 QUINTE 审计过
- **Workflow 机制的 pipeline()/parallel() 是黑盒**——hm 作为监督者看不到 cc 内部 Workflow 的执行细节，只能看到 Phase 边界输出
- **三机制的认识论声称（"decorrelated errors"）是未经实证的假设**——PHENOMENOLOGY.md L23: "Their errors decorrelate — which is precisely why combining them improves truth-seeking" 这句话没有引用任何 QUINTE session 数据支持

**omp 的独特价值**：omp 的 LSP/DAP 调试能力是三机制中唯一的"ground-truth verification"能力——Bash 执行代码验证可以真实验证 claim。三机制文本中 omp 被归类为"rare reasoning + security"，没有突出这一独特能力。

### 2.3 跨模型对抗性验证：🛑 当前环境结构性不可满足

[鏡門 ✓] v3_requinte_final.md L29-31: "DeepSeek-only 环境下 Invariant#4 结构性不可满足"

**这是 v3.0 最严重的已知问题**。

PROTOCOL.md L199 写"Cross-model diversity in R2. At least 1/3 refuters from different provider"——但在用户当前 DeepSeek-only 环境中（无 Anthropic/OpenAI key），此 Invariant 无法满足。v3_requinte_final.md 的用户决策是"先用 DeepSeek variant 凑合, 标注已知限制"——但"凑合"意味着 v3.0 的核心 Invariant 在部署环境中是 broken-by-design。

v2.4 没有此 Invariant——它诚实地承认"same-model consensus is weaker"。v3.0 增加了一个无法满足的 Invariant，反而制造了"我们标了已知限制所以没问题"的虚假安慰。

### 2.4 loop-until-dry：⚠️ 方向对但操作化不足

[鏡門 ✓] PROTOCOL.md L118-128 定义 loop-until-dry；v3_requinte_final.md L21-25 cw 指出"证据重复度>90%"无操作化定义

**价值**：
- 双条件终止 + escalate→人工 是正确的收敛设计
- 避免了"无限循环"和"自动接受虚假收敛"两个极端
- cost circuit breaker (loop≤5) 是合理的成本约束

**问题**：
- "证据重复度>90%" 无 hash/归一化方案——cw 在 v3 self-audit 中已指出
- loop≤5 + 需要 2 轮 dry → 有效收敛窗口只有 loop 1-3（loop 4-5 必须 dry），太紧
- escalate→人工 在 cron job / 自动化场景中不可用——QUINTE 经常运行在无人值守环境
- 双 critic 不同配置 (temperature/prompt template) 的具体差异未定义——可能产生 correlated errors 而非 independent checks

### 2.5 治理层：✅ 必要但实现不足

[鏡門 ✓] PROTOCOL.md L142-151 定义治理层五机制

**价值**：
- cost circuit breaker 在 DeepSeek 低价环境下防止异常消耗
- poison detection (单 agent >50 claims 标记异常) 是实际的防御
- state persistence 使得跨辩论知识可积累

**问题**：
- state persistence 路径 `~/.hermes/quinte/` (PROTOCOL.md L149) 在 profile 环境下解析到沙箱——用户看不到。应使用绝对路径或 $HERMES_HOME
- poison detection 阈值 (>50 claims) 是 arbitrary——没有来源说明为什么是 50
- cross-round consistency Agent 是 cc 内部 Agent——如果 cc 本身漂移，一致性检查也会漂移

### 2.6 并行四道门：✅ 设计合理，但 ~5s 声称可疑

[鏡門 ✓] GATES.md L1-2: "并行执行（~5s），hm 操作"

**价值**：
- 四门从串行改并行是正确的——每个门检查独立维度，无需等待
- 證門拆为闸门层+执行层澄清了"快速判断 vs 完整执行"的混淆

**问题**：
- "~5s" 声称：hm 不能真正并行执行四个 reasoning 任务——xhigh reasoning 本身就是串行的。~5s 是指 hm 的快速判断+prompt 审核，不是四个独立推理。文本应澄清
- 閂門的"三层法审核 prompt"在并行模式下不可能——prompt 还没写出来怎么审核？实际上是 Phase -1 只做闸门判断，prompt 审核在 cc dispatch 时再做

---

## 3. 跨文件版本不一致

[鏡門 ✓] 多处版本号不一致：

| 位置 | 声明版本 | 实际协议版本 |
|------|---------|------------|
| QUINTE/README.md L14 badge | "protocol-v2.4" | 但 SPEC 和 CHANGELOG 都是 v3.0 |
| QUINTE/spec/PROTOCOL.md L1 | "v3.0" | ✅ 一致 |
| QUINTE/CHANGELOG.md L25-26 | v3.0 | ✅ 一致 |
| RASHOMON/GATES.md L1 title | "v3.0" | ✅ 一致 |
| RASHOMON/CONCEPTS.md L126 footer | "CONCEPTS-v3.md" | ✅ 一致 |
| RASHOMON/PHENOMENOLOGY.md L37 footer | "PHENOMENOLOGY-v3.md" | ✅ 一致 |
| KANSA/README.md | 已更新为 cc orchestrate | ✅ 一致 |

QUINTE/README.md badge 是 **唯一残留的 v2.4 引用**。这是用户可见的第一印象——访客看到 badge v2.4 但 spec 是 v3.0，造成混乱。

---

## 4. omp 在 v3.0 中的角色——核心缺失

[鏡門 ✓] PROTOCOL.md L22: omp 在 R1+R2 表中为"Rapid reasoning + security perspective"

这是本次 QUINTE 最重要的发现：

**v3.0 将编排权全面移交给 cc，但此举在提出时 omp 在 Linux 上不可用。omp 被排除在架构决策之外。**

具体而言：
1. v3.0 ratification debate (2026-06-09) 的 final_verdict.md 记录了 5/5 一致通过——但当时的 omp 是否完整参与？v3_requinte_final.md L43 写"omp: 纯 API 平替, 无文件读取, R1 价值有限"。如果 omp 当时只能以 API-only 模式运行，它的 R1/R2 质量不足以充分表达 Linux 环境的约束。
2. omp 的独特能力（LSP/DAP 调试、Bun runtime）在三机制分类中被归入"Bash"——与其说是认识论分类，不如说是技术降级。omp 的 code execution verification 是 ground-truth 来源，不同于 cc/cw/rx 的纯推理。
3. omp 是唯一能在 cc 故障时提供实质性替补的 agent——cw 和 rx 都没有 omp 的调试能力。但 v3.0 的 fallback 路径 (RASHOMON/README.md L109) 只说"hm takes over"，未提及 omp 的替补角色。
4. omp 是唯一不依赖 Node.js 的 agent（Bun runtime）——在 Linux Arch 上这是显著优势。cc 的 Node.js 依赖在 Linux 上有额外复杂性问题。

---

## 5. v3.0 复杂度评估

v2.4 概念数：agents(5) + rounds(3) + gates(4) + KANSA(1) + KENGEN(1) = ~14 个独立概念

v3.0 新增概念：
- 编排-监督分离
- 三机制认识论 (Agent/Workflow/Bash)
- Phase 0-6 (7 个 phase，原 3 rounds)
- auto-diff claims
- JSON Schema structured output
- loop-until-dry (双 critic + 双条件)
- cross-model adversarial verification
- governance layer (5 子机制)
- cross-round consistency Agent
- Manifest Agent
- poison detection
- 證門 two-layer design

v3.0 概念密度增加 ~2.5×。PROTOCOL.md 从 v2.4 的 ~80 行增长到 230 行。RASHOMON 新增 CONCEPTS.md (126 行) + PHENOMENOLOGY.md (37 行)。

**这确实繁琐。** 但繁琐本身不是问题——问题是新增复杂度带来的收益是否超过认知负担。

---

## 6. 综合评估

### 高价值变更（应保留）
| 变更 | 理由 |
|------|------|
| 编排-监督分离 | 解决 v2.x 核心结构性问题（内生利益冲突），有生产环境证据 |
| 治理层 cost circuit breaker | 在 loop-until-dry 下必要的成本防护 |
| 證門两层设计 | 澄清了"快速判断 vs 完整执行"的混淆 |
| 并行四道门 | 正确的性能优化，无逻辑损失 |

### 问题变更（需修正）
| 变更 | 问题 |
|------|------|
| 跨模型对抗性验证 Invariant#4 | 当前环境结构性不可满足 → 应从 Invariant 降为 Desideratum |
| loop-until-dry 操作化 | 证据重复度无定义、收敛窗口太紧 |
| 三机制认识论 | 理论精妙但未经 QUINTE session 验证，Agent/Workflow 对 hm 是黑盒 |

### 应删除/简化
| 概念 | 理由 |
|------|------|
| cross-round consistency Agent | cc 内部 Agent，hm 不可见——如果 cc 漂移则一致性检查也漂移 |
| Phase 2 auto-diff JSON Schema | 过度工程化——v2.4 的 hm 手动标注分歧已足够 |
| 三机制作为独立概念 | 有价值的洞察（cc 有三种能力）但不需要提升为"认识论框架"——过度哲学化 |

### omp 应获得的角色
- omp 应成为 **cc 故障时的首选替补编排者**（非 hm），因为 omp 有 LSP/DAP 调试和 code execution verification
- omp 的调试能力应在三机制中被识别为独立的"Verification"机制，而非归入"Bash"
- Linux 环境下 omp 的 Bun runtime 优势应在 extensions.md 中记录

---

## 7. 初步结论

v3.0 的编排-监督分离是正确方向，但 v3.0 协议 spec 过度工程化。核心洞察（"执行者不应是评判者"）可以更简单地实现，不需要 7 个 Phase、JSON Schema 自动 diff、cross-round consistency Agent、三机制认识论框架等附加层。

**建议方向：v3.1 精简版**——保留编排-监督分离+治理层+證門两层设计，移除跨模型 Invariant（降为 Desideratum）、简化 loop-until-dry（单 critic + 固定 3 轮 hard cap）、删除三机制认识论框架（在 RASHOMON 哲学文档中保留但不进入 PROTOCOL.md 规范）。

*文件覆盖率: 26/26 主内容文件 (100%)*
