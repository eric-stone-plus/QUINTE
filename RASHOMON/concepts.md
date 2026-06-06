# 核心概念 — Core Concepts

QUINTE 从罗生门隐喻中提取的操作性概念。这些是术语词汇，不是协议规范——但它们为 QUINTE 的机制提供了命名和直觉。

---

## 罗生门深度（Rashomon Depth）

**定义**: 不是 agent 数量，而是实际产生的独立视角数。

```
Rashomon Depth = |{ genuinely different reasoning paths }|
```

**判断标准**:
- 同模型换 prompt 参数 → Depth = 1（换面具，不是新证人）
- 不同模型、不同工具链、不同关注偏好 → Depth 增加
- R2 交叉审本身也贡献 Depth——换个角度看别人的分析

**经验法则**: 如果四方 R1 输出惊人一致且无人说"不确定"，**这可能是共享盲区，不是共识**。罗生门里如果四个人说的完全一样，反而可疑。

---

## 藪の中指数（Yabu no Naka Index）

**定义**: R1 各方陈述的不一致度。取自芥川龙之介原著名《藪の中》（竹林中）。

```
YNI = 1 - (各方论点交集 / 各方论点并集)
```

**解读**:
- YNI ≈ 0：高度一致 → ⚠️ 可能共享盲区，R2 做确认性审计
- YNI ≈ 0.3–0.5：健康分歧 → R2 标注争议
- YNI > 0.7：高度分散 → 可能原问题本身模糊，退回雨門

---

## 黑泽明检查（Kurosawa Check）

**定义**: R2 强制交叉审。QUINTE 的核心机制。

**隐喻**: 黑泽明拍《罗生门》不只是让四个人各说各的。真正的力量在于：**观众在对比供述的过程中，自己推演出接近真相的理解。** R2 就是这个"对比"环节。

**操作**: 每个 agent 审其他所有人的 R1 输出（不审自己），找出：
1. 四方一致但可能漏看的地方
2. 两方一致、一方异议的边界
3. 各方完全分歧的核心

**规则**: R2 永不跳过。就像你无法看完一个人的供述就下结论——必须看完所有人的，然后对照。

---

## 术语表

| 术语 | 日语 | 读音 | 含义 |
|------|------|------|------|
| Rashomon Depth | — | — | 独立视角数 |
| Yabu no Naka Index | 藪の中 | やぶのなか | R1 分歧度 |
| Kurosawa Check | — | — | R2 强制交叉审 |
| Amamon | 雨門 | あまもん | Ambiguity gate |
| Shōmon | 證門 | しょうもん | QUINTE gate |
| Kan'nukimon | 閂門 | かんぬきもん | Anti-Drift gate |

## 与协议术语的对应

本文件中的概念是**操作性隐喻**，非协议规范。对应协议术语见 [spec/PROTOCOL.md](../spec/PROTOCOL.md)：

| 隐喻概念 | 协议术语 | PROTOCOL.md 位置 |
|---------|---------|-----------------|
| Rashomon Depth | agent 独立视角质量 | §3 Invariants (no model-tier degradation) |
| Yabu no Naka Index | R2 分歧标注 | §2.3 Final Verdict (adjudication rules) |
| Kurosawa Check | R2 强制交叉审 | §2.2 Cross-Review (R2 is mandatory) |
| Amamon | Ambiguity gate | SOUL.md (persona-level, not in PROTOCOL.md) |
| Shōmon | QUINTE discipline | §1-5 (full protocol) |
| Kan'nukimon | Agent dispatch anti-drift | §7 Agent Dispatch Requirements |
