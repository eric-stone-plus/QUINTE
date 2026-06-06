# Hermes Agent — 三道强制门

**日期**: 2026-06-07 | **QUINTE 裁定**: hm+cc+cw+omp+rx | **协议**: v2.4

---

## 概述

SOUL.md 中设置三道强制门，按逻辑优先级排列，每 session 主动注入。三道门共同构成了 Hermes Agent 的行为护栏——在行动之前强制通过清晰度检查、结构化辩论和防漂移约束。

---

## 门 1：Ambiguity Gate（最优先）

**位置**: SOUL.md § Ambiguity gate  
**触发**: 用户问题模糊不清——不确定意图、范围、具体指哪个  
**动作**: 必须先 `clarify` 反问确认，再行动  

**设计理由**: 与其花半小时跑完 QUINTE 发现理解错了，不如多问一句。此门先于一切——连 QUINTE 本身也不能在模糊问题上启动。

**示例**:
- ❌ 用户说"修一下那个" → 直接开始修
- ✅ 用户说"修一下那个" → clarify "是指 demo 脚本的 agent 大小写 bug？"

---

## 门 2：QUINTE Discipline

**位置**: SOUL.md § QUINTE discipline  
**触发**: 任何可能被用户依赖的结论——push、配置修改、报告、skill 更新、协议变更、台账、报表、经济分析、合同解释等  
**动作**: 完整 R1+R2+R3（hm+cc+cw+omp → +rx 交叉审 → hm 终裁）

**关键约束**:
- **不自判简化**: Hermes 不自行判断"这个简单不需要 QUINTE"——跳过权在用户
- **可验证性**: 终裁必须列出各 agent 文件名和具体论点引用。无引用 = 未执行 = 结论不可信
- **R2 不跳过**: 四方一致也可能是共享盲区
- **rx 仅 R2**: run 模式无工具调用，纯推理交叉裁判
- **R3 裁决**: ≥3/5 确认，2/5 分歧，≤1/5 驳回

**例外**: 严格确定性单工具读取（ls、stat、cat 已知路径），零推理可跳过。

---

## 门 3：Agent Dispatch Anti-Drift

**位置**: SOUL.md § Agent dispatch anti-drift  
**触发**: 每次向 cc/cw/omp/reasonix 发送 prompt  
**动作**: 三层法——① task-first（具体任务放最前）、② 语义隔离（"ONLY Y" 非 "NOT X"）、③ 强制首行复述（`TASK: [restatement]`）

**设计理由**: LLM agent 看到 prompt 中的关键词可能激活训练数据中的错误概念关联（概念命名空间碰撞）。否定指令（"NOT X"）无效——模型需先激活 X 才能理解否定。三层法从源头防止漂移。

**协议依据**: QUINTE `spec/PROTOCOL.md` §7 Agent Dispatch Requirements

---

## 执行顺序

```
用户问题
  │
  ▼
┌─────────────────────┐
│ 门 1: Ambiguity     │  模糊？→ clarify 反问
│ 最优先              │  清晰？→ 通过
└─────────────────────┘
  │
  ▼
┌─────────────────────┐
│ 门 2: QUINTE        │  结论性输出？→ R1+R2+R3
│ 强制                │  单工具读取？→ 跳过
└─────────────────────┘
  │
  ▼
┌─────────────────────┐
│ 门 3: Anti-Drift    │  分发 prompt 给外部 agent？
│ 强制                │  → 三层法包装
└─────────────────────┘
  │
  ▼
输出
```

---

## 可验证性

用户可通过以下信号判断每道门是否被执行：

| 门 | 检测信号 |
|----|---------|
| Ambiguity | 输出中是否出现 `clarify` 反问（而非直接行动） |
| QUINTE | 终裁中是否列出各 agent 文件名和具体论点引用 |
| Anti-Drift | prompt 是否以具体任务开头、用 "ONLY Y" 表述、首行要求复述 |

---

## 产物

| 文件 | 位置 |
|------|------|
| SOUL.md（三道门） | `~/.hermes/profiles/technical/SOUL.md` |
| 协议规范 | `QUINTE/spec/PROTOCOL.md` §7 |
| 完整反漂移参考 | `references/anti-drift-layered-defense.md` |
| 本报告 | `~/Downloads/hermes-three-gates-report` |
