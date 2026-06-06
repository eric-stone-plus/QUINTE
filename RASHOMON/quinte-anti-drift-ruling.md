# QUINTE 裁定：LLM Agent 概念命名空间碰撞 — 通用防漂移方案

**日期**: 2026-06-07 | **裁定**: hm + omp + cw + rx 四方一致 | **状态**: 终裁通过

---

## 1. 问题

LLM agent（cc、cw 等）在收到包含特定关键词的 prompt 时，训练数据中的强关联会覆盖显式指令，导致 agent 漂移到无关领域。

**已确认案例**：
- cc 和 cw 收到含 "Hermes" 的 prompt → 自动激活 hermes-desktop FAQ/源码分析
- 否定指令（"NOT hermes-desktop"）完全无效
- 同日 4 次漂移，反漂移 guard 形同虚设

**根因**：`NOT X` 要求模型先激活 X 的概念才能理解否定——激活即漂移。

---

## 2. QUINTE 过程

| Agent | R1 结论 | 关键论点 |
|-------|---------|----------|
| **hm** | 三层组合 | aliasing + output gating + forced restatement |
| **omp** | task-first + isolation + restatement | 零基础设施，三者组合最强。提供具体 before/after 示例 |
| **cw** | 4-layer defense | 14 种技术，从 pre-dispatch 到 recovery。新增 embedding 筛查、XML 模板、碰撞日志 |
| **rx** (R2) | forced restatement 为核心 | "最 impactful 的单点改进"，确认三方收敛 |
| **cc** | E = B+C+D 组合 | 排除 aliasing（脆弱/维护负担），pipeline: prevent → redirect → catch |

**R3 裁决**: 4/4 全票通过。≥3/5 确认为终裁通过。

---

## 3. 方案：三层防漂移

### Layer 1: Task-First（任务前置）

具体任务放在 prompt 最前面，模型先锚定任务框架。

```
旧: "You are an agent. NOT desktop support. Audit the agent system..."
新: "Read /tmp/file.md. Your ONLY task: verify [specific question]..."
```

### Layer 2: Semantic Isolation（语义隔离）

用 `ONLY Y` 替代 `NOT X`，建立正向身份边界。

```
旧: "Do NOT answer questions about desktop apps."
新: "You are EXCLUSIVELY a file auditor. Zero connection to any desktop application."
```

### Layer 3: Forced Restatement（强制重述）

第一行输出 `TASK: [复述]`。漂移在第一句就能检测到。

```
输出: "TASK: Audit whether the agent uses flash models by checking config." ← 正确
输出: "TASK: Audit the desktop app README and build scripts."            ← 立即废弃
```

---

## 4. 部署路线

| 阶段 | 措施 | 成本 |
|------|------|------|
| **今天** | Task-first + ONY Y + forced restatement（改 prompt 模板） | 零 |
| **本周** | 关键词别名 MAP + 输出首行校验 + XML 模板 | 轻度工程 |
| **本月** | Embedding 碰撞筛查 + 碰撞日志反馈闭环 | 需要基础设施 |

---

## 5. 适用范围

不是 Hermes 特有的问题。任何通过 prompt 调用外部 LLM agent 的系统都可能遇到概念命名空间碰撞。方案是通用的——替换碰撞词即可适用于任何 agent/产品组合。

---

## 6. 文件变更

| 文件 | 操作 | 说明 |
|------|------|------|
| `skills/multi-agent-debate/SKILL.md` | 已更新 | Pre-R1 Checklist #5 替换为三层法 |
| `skills/multi-agent-debate/references/anti-drift-layered-defense.md` | 新建 | 通用方案参考文档 |
| `skills/multi-agent-debate/references/cc-cw-hermes-desktop-drift.md` | 已有 | 本案例的具体记录 |

---

## 7. Push 建议

| 目标 | 内容 | 建议 |
|------|------|------|
| `hermes-core-rules-mac` | SKILL.md + 参考文档 | ✅ 应 push（本 profile 的 QUINTE 实现） |
| `quinte` (PROTOCOL.md) | 防漂移作为 prompt 工程指南 | ⚠️ 待裁：通用方法论是否应写入协议规范 |
| `quinte` (POSTMORTEM.md) | 本次漂移案例 | ⚠️ 待裁：案例属于协议级教训还是实现细节 |
