# Mac 全量 QUINTE 重跑备忘 — 2026-06-09

## 为什么要重跑

本机 (Arch Linux, Celeron N4020, 无 AVX2) QUINTE 有硬件限制：
- omp: 纯 Python HTTP 平替, 无 LSP/DAP/MCP 工具, R1 不能读文件
- cc: 首轮大 prompt 超时 → 拆为 2 轮
- rx: R2 只喂了摘要非完整 R1 原文本

Mac 上真 omp (Bun 版) + 完整 cc + 全量 rx + cw → 完整五方 QUINTE。

## 待审计的 repo 和文件

| Repo | 文件 | 本地路径 (Mac) | 本次改动 |
|------|------|----------------|---------|
| QUINTE | spec/PROTOCOL.md | clone from github | 證門 Action 单层→两层 + 执行说明 |
| RASHOMON | README.md | clone from github | 新增 "The Four Gates · 四道門" 独立章节 |
| RASHOMON | GATES.md | clone from github | footer 版本号 v3.1→v3.0 |
| KANSA | README.md | clone from github | "lead arbiter (hm)" → cc orchestrate + hm veto |
| [implementation repo] | skills/multi-agent-debate/SKILL.md | 本地 repo | 已是 v3.0 (cc 2026-06-09 推送) |

## 审计问题 (四题并行)

Q1: v3.0 框架自洽 — PROTOCOL vs SKILL vs GATES 有无矛盾？
Q2: loop-until-dry 阈值 — dry=2/claims≤100/反驳≤50/loop≤5 有意义？
Q3: 跨模型对抗性验证 — DeepSeek-only 下 "≥1 不同 provider" 怎么实现？
Q4: KANSA README vs PROTOCOL Phase 6 — 描述一致吗？

## 本机 QIUNTE v3.0 自审发现 (供 Mac 交叉验证)

- **證門两层设计**: PROTOCOL 单层 vs GATES 两层 → 已修
- **GATES footer 版本号**: v3.1 vs v3.0 → 已修
- **loop-until-dry**: cw 发现 "证据重复度>90%" 无操作化定义；loop≤5 与收敛窗口冲突
- **跨模型**: cw CRITICAL — 无 provider 可用性检查、dispatch 无 --model flag、无降级模式
- **KANSA spec fork**: cw 发现 6 维度 mismatch (Agent 身份/输出物/架构关系/毒化检测/registry/log)
- **rx 交叉裁判**: 支持 cw 判断 — cc 的 "部分匹配" 低估了 KANSA 分叉程度

## 本机 QUINTE 文件归档

```
/tmp/quinte-audit/
├── cc_q1.md          # cc Q1 审计 (PROTOCOL vs GATES 證門矛盾)
├── cc_q234.md        # cc Q2-Q4 审计 (loop/跨模型/KANSA)
├── cw_r1.md          # cw R1 深度审计 (6 维度 mismatch 表)
├── omp_r1.md         # omp R1 (无文件读取, 推断为主)
├── rx_r2.md          # rx R2 交叉裁判
└── v3_requinte_final.md  # hm 终裁报告
```

## Mac 执行步骤

```bash
# 1. clone repos
git clone https://github.com/eric-stone-plus/QUINTE.git /tmp/quinte-mac
git clone https://github.com/eric-stone-plus/RASHOMON.git /tmp/rashomon-mac
git clone https://github.com/eric-stone-plus/KANSA.git /tmp/kansa-mac

# 2. 用 hm 发起: "quinte 一下 v3.0 框架自洽性, 审计 QUINTE+RASHOMON+KANSA+SKILL.md"

# 3. hm Phase -1 四道门 → 上下文注入 → cc Workflow 主编排

# 4. 交叉验证本机发现的 3 个 HIGH+ 问题
```
