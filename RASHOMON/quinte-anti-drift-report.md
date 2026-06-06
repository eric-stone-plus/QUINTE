# QUINTE Anti-Drift Protocol — 裁决报告

**日期**: 2026-06-07 | **QUINTE 轮次**: 2 轮（R1+R2+R3 完整执行）| **版本**: 2.4

---

## 议题

LLM agent（cc、cw、omp）在执行 QUINTE 辩论时，因 prompt 中的产品名关键词触发训练数据中的错误概念关联，导致输出漂移到无关领域。需要通用、可复用的防漂移方案。

## 参与方

| Agent | Round 1 | Round 2 | 贡献 |
|-------|---------|---------|------|
| **hm** | 6 种候选方案 | 交叉审 + 确认收敛 | 方案框架 + 终裁合成 |
| **omp** | 6 种方案排名 + 三层推荐 | — | 最清晰的零基础设施方案 + 具体示例 |
| **cw** | 14 种方案（6 critique + 8 new）| — | 最全面分析 + 四层渐进部署 + 实施时间线 |
| **cc** | Round 1 超时 → Round 2 重试成功 | — | 确认 E（B+C+D 组合），排除 A（别名脆弱） |
| **rx** | —（R1 不参与） | 确认三方收敛 + forced restatement 为单一最大影响项 | 独立验证 |

## 投票（v2.3 规则）

| 议题 | hm | omp | cw | cc | rx | 结果 |
|------|-----|-----|-----|-----|-----|------|
| 强制重述（forced restatement）为核心机制 | ✓ | ✓ | ✓ | ✓ | ✓ | **5/5 一致** |
| 分层防御优于单一技术 | ✓ | ✓ | ✓ | ✓ | ✓ | **5/5 一致** |
| 关键词别名（aliasing）脆弱不推荐 | — | ⚠️ | ⚠️ | ✗ | — | **不支持** |
| Task-first + 语义隔离 + 强制重述 = 最佳组合 | ✓ | ✓ | ✓ | ✓ | ✓ | **5/5 一致** |

## 终裁方案

### 三层防漂移（今天可部署，零基础设施）

| # | 技术 | 原理 | 实现 |
|---|------|------|------|
| 1 | **Task-first** | 具体任务放 prompt 最前面，模型从左到右处理时先锚定任务框架 | 改 prompt 模板顺序 |
| 2 | **语义隔离** | "ONLY Y" 替代 "NOT X"，建立正向身份而非否定关联 | 改措辞 |
| 3 | **强制重述** | 要求第一行输出 `TASK: [复述]`，漂移在第一句就能检测 | 加一行输出要求 |

### 渐进部署路线

| 阶段 | 时间 | 内容 |
|------|------|------|
| **立即** | 今天 | 三层法改 prompt 模板 |
| **短期** | 本周 | 关键词别名映射 + 输出首行校验 |
| **中期** | 本月 | embedding 碰撞筛查 + 碰撞日志反馈闭环 |
| **研究** | 持续 | token 级 logit 压制、attention head 干预 |

### QUINTE 流程变更

1. cc/cw/omp 的 R1 prompt 模板改为 task-first + 语义隔离 + 强制重述
2. hm 在 R2 前校验每个 agent 输出首 5 行——命中漂移关键词则废弃该 agent 投票
3. `multi-agent-debate` SKILL.md 升级至 v2.4

## 关键发现

1. **否定指令完全无效** — "NOT hermes-desktop" 写三遍，cc 和 cw 照漂不误
2. **完全移除触发词有效** — cw prompt 不含 "Hermes" 时正常完成
3. **cc tee 输出对 poll 不可见** — 197s 时 poll 显示空，但文件已有 187 行内容。不要仅凭 poll 判定
4. **漂移是通用问题** — 不是 Hermes 特有，任何 LLM agent 都可能因命名空间碰撞而漂移

## 产物

| 文件 | 位置 |
|------|------|
| SKILL.md v2.4 | `~/.hermes/profiles/technical/skills/multi-agent-debate/SKILL.md` |
| 通用协议 | `~/.hermes/profiles/technical/skills/multi-agent-debate/references/anti-drift-layered-defense.md` |
| 本报告 | `~/Downloads/quinte-anti-drift-report.md` |
