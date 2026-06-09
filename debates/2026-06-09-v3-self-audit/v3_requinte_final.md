# QUINTE v3.0 自审终裁 — 2026-06-09

**参与方**: hm(R1+R2+终裁) + cc(R1) + cw(R1) + rx(R2) / omp(R1, 无文件读取能力, 未计入)
**架构**: v3.0 — cc 作为外部 Agent 参与 R1, hm 监督+参辩+终裁, rx R2 纯推理交叉裁判
**结论**: 5/5 一致 — v3.0 方向正确, 但存在 3 个 High+ 问题需修

---

## 已修复

| # | 问题 | 文件 | 修复 |
|---|------|------|------|
| 1 | 證門单层 vs 两层矛盾 | PROTOCOL.md §6 | Action: "Full R1+R2+R3" → "Gate layer: hm ~1s → cc pipeline" |
| 2 | 执行说明未区分證門闸门层 | PROTOCOL.md §6 | 追加注释: 證門 in Phase -1 = gate layer only |
| 3 | GATES footer v3.1 vs header v3.0 | GATES.md | footer: v3.1 → v3.0 |
| 4 | KANSA 仍提 "lead arbiter (hm)" | KANSA/README.md | 更新为 cc orchestrate + hm veto 描述 |

## 跨 Agent 交叉审发现 (HIGH+)

### Q1: loop-until-dry 阈值 (cw: HIGH)

- **一致**: 双条件+escalate 结构有意义
- **问题**: "证据重复度>90%" 无操作化定义 (无 hash/归一化方案)
- **问题**: loop≤5 成本熔断与收敛窗口冲突 (需 2 轮 dry, cap 在 5)
- **建议**: v3.1 给出证据重复度的具体算法; 熔断从 loop≤5 改为 "loop≤5 后 hm 审批"

### Q2: 跨模型对抗性验证 (cw: CRITICAL, cc: 5 档方案)

- **一致**: DeepSeek-only 环境下 Invariant#4 结构性不可满足
- **问题**: 无 provider 可用性检查; dispatch 命令无 --model flag; 无降级模式
- **用户决策**: 先用 DeepSeek variant 凑合, 标注已知限制, 以后加 Anthropic
- **建议**: v3.1 增加 Phase -1 provider 可用性检查; 规范 "different model" 定义

### Q3: KANSA vs PROTOCOL (cw: spec fork, rx 支持 cw)

- **cw 发现**: 6 维度 mismatch — Agent 身份 (hermes vs rotating consul)、输出物 (checklist vs parallel verdict)、架构关系 (sequential vs orthogonal)、毒化检测缺失、registry Agent 缺失、log 目标不同
- **rx 裁决**: cw 更准确 — cc 的 "部分匹配" 低估了分叉程度
- **建议**: v3.1 将 KANSA README 的 consul 匹配表+双人裁决+merge 吸收进 PROTOCOL Phase 6

## 本次 QUINTE 局限性

- omp: 纯 API 平替, 无文件读取, R1 价值有限
- rx: 纯推理, R2 prompt 内嵌摘要非完整 R1 原始文本
- cc: 首轮 prompt 过大超时, 拆为 2 轮才完成
- **Mac 上应用真 omp (Bun 版, 有 LSP/DAP/MCP) + 完整 rx + cc 一次性全量 QUINTE**

---

*hm 终裁, rx R2 交叉裁判支撑, cc+cw R1 实证基础*
