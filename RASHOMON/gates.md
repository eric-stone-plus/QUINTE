# 三道门 — The Three Gates

QUINTE 的三道强制门，按逻辑优先级排列。每道门对应罗生门叙事中的一个意象。

---

## 门 1：雨門 Amamon · Ambiguity Gate

**罗生门隐喻**: 樵夫在雨中走进罗生门——进入之前，先确认你到底要审什么。

**触发**: 用户问题模糊不清——不确定意图、范围、具体指哪个。

**动作**: 必须先 `clarify` 反问确认，再行动。

**设计理由**: 与其花半小时跑完 QUINTE 发现理解错了，不如多问一句。

**SOUL.md 位置**: § Ambiguity gate（强制 — 最优先）

**命名**: 雨（あめ / ame）= rain。罗生门开场是大雨。雨門是入口——不清晰不进入。

---

## 门 2：證門 Shōmon · QUINTE Gate

**罗生门隐喻**: 证人在城门内各自供述，交叉对质。真相不在任何一个供述里，在对比中浮现。

**触发**: 任何可能被用户依赖的结论——push、配置修改、报告、skill 更新、协议变更、台账、报表、经济分析、合同解释等。

**动作**: 完整 R1+R2+R3（hm+cc+cw+omp → +rx 交叉审 → hm 终裁）。

**关键约束**:
- 不自判简化：Hermes 不自行判断"这个简单不需要 QUINTE"
- 可验证性：终裁必须列出各 agent 文件名和论点引用
- R2 不跳过：四方一致也可能是共享盲区
- rx 仅 R2：run 模式无工具调用，纯推理交叉裁判
- R3 裁决：≥3/5 确认，2/5 分歧，≤1/5 驳回

**SOUL.md 位置**: § QUINTE discipline（强制）

**命名**: 證（しょう / shō）= testimony, evidence。證門是证人对质的场所。

---

## 门 3：閂門 Kan'nukimon · Anti-Drift Gate

**罗生门隐喻**: 门闩——不许证人串供。每个人的供述必须独立，不受外部关联污染。

**触发**: 每次向 cc/cw/omp/reasonix 发送 prompt。

**动作**: 三层法——
1. **Task-first**: 具体任务放 prompt 最前面
2. **语义隔离**: "ONLY Y" 替代 "NOT X"，建立正向身份
3. **强制首行复述**: 要求 `TASK: [restatement]`，漂移在第一句即可检测

**设计理由**: LLM agent 看到 prompt 中的关键词可能激活训练数据中的错误概念关联（概念命名空间碰撞）。否定指令无效——模型需先激活被否定概念才能理解否定。

**SOUL.md 位置**: § Agent dispatch anti-drift（强制）

**命名**: 閂（かんぬき / kan'nuki）= bolt, latch。閂門是门闩——防止外部污染进入证词。

---

## 边界说明

本文件是**设计哲学**，非协议规范。三道门的可执行定义在 [spec/PROTOCOL.md](../spec/PROTOCOL.md)：
- Ambiguity gate 的触发条件见 SOUL.md § Ambiguity gate
- QUINTE gate 的 R1+R2+R3 机制见 PROTOCOL.md §1-5
- Anti-Drift gate 的三层法见 PROTOCOL.md §7 Agent Dispatch Requirements

此处仅提供隐喻解释和命名来源。所有规范性约束（MUST/SHALL/SHOULD）以 PROTOCOL.md 为准。

---

## 执行流程

```
用户问题
  │
  ▼
雨門 ── 模糊？→ clarify 反问
  │ 清晰
  ▼
證門 ── 结论性输出？→ R1+R2+R3
  │
  ▼
閂門 ── 分发 prompt？→ 三层法包装
  │
  ▼
输出
```
