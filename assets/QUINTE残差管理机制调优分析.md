> **Historical analysis only (non-normative).** Current product version is **0.1.1**. Read `GROK_HANDOFF.md` first.

# QUINTE 残差管理机制调优分析

**日期**: 2026-07-14
**目的**: 供codex参考，用于QUINTE残差机制的设计和实现
**来源**: 基于QUINTE仓库源码分析 + KING LOONG案例复盘

---

## 一、QUINTE架构概述

QUINTE是一个协议强制的五方对抗性辩论CLI，用于在采纳结论前暴露分歧、遗漏、证据缺口和未解决风险。

### 核心组件

| 文件 | 职责 |
|------|------|
| `src/run.rs` (3670行) | 状态机、R1/R2/R3调度、合并逻辑 |
| `src/adapters.rs` (1974行) | 五个适配器的调用、输出解析、隔离控制 |
| `src/model.rs` (387行) | 数据模型：Brief, Policy, LaneOutput, Residual等 |
| `src/schema.rs` (61行) | JSON Schema验证 |
| `src/store.rs` (687行) | 持久化、文件锁、进程管理 |
| `src/cli.rs` (481行) | CLI命令解析 |
| `src/policy.rs` (163行) | 策略加载和验证 |

### 状态机

```
queued → preflight → r1_running → r1_gate
     → r2_packet → r2_running → r2_gate
     → r3_cc → waiting_hm
     → merging → completed
```

### 残差schema (lane-output.schema.json)

```json
{
  "id": "string (pattern: ^[A-Za-z0-9._-]{1,64}$)",
  "severity": "LOW|MEDIUM|HIGH|CRITICAL|P0",
  "residual_type": "string (自由文本)",
  "source": "string",
  "finding": "string",
  "evidence_refs": ["string"],
  "disposition": "verified|falsified|unresolved|escalated|discarded",
  "required_closure": "string",
  "closure_state": "open|closed|blocked|waived|not_applicable",
  "closure_evidence": ["string"],
  "scope": "string"
}
```

---

## 二、残差的本质

### 数学定义

残差 = 观测值 - 模型预测值 (ε = y - ŷ)

在QUINTE中：
残差 = 问题的全部复杂性 - 五方共识能覆盖的部分

### 四重身份

1. **数学身份**: 观测值与模型预测的差异
2. **工程身份**: 验证后仍未解决的证明义务 (形式验证语境)
3. **风险管理身份**: 实施控制措施后仍存在的残余风险 (ISO 31000)
4. **认识论身份**: 系统对自身无知的诚实声明

### 与相关概念的区分

| 概念 | 定义 | 与残差的关系 |
|------|------|-------------|
| 误差(Error) | 观测值与真实值的差异 | 误差不可知，残差可计算 |
| 不确定性(Uncertainty) | 对结论的信心不足 | 残差是不确定性的结构化表达 |
| 异议(Dissent) | 两个arbiter对结论的不同判断 | 异议是关于"答案"的分歧，残差是"问题"本身 |
| 风险(Risk) | 未来可能发生的问题 | 残差是已实现的风险，不是潜在的风险 |

---

## 三、四个调优方向

### 3.1 残差识别的准确性

**当前机制**:
- R1: 五个party独立分析，各自产出`LaneOutput.residuals[]`
- R2: 匿名交叉审查，可能识别新残差
- R3: Hermes和Auditor B产出最终残差

**问题**:

1. **同模型家族盲区**: 五个party都用`mimo-v2.5-pro`，是同一家族的行为扰动
   - 源码: `model.rs` 第11行 `pub const TEXT_MODEL: &str = "mimo-v2.5-pro";`
   - PROTOCOL.md: "Same-family agreement may represent a shared blind spot rather than confirmation."

2. **无领域checklist注入**: brief的context字段是自由文本，无法结构化注入"必须检查的项目"

3. **无遗漏检测机制**: 五方都遗漏的残差，QUINTE无法发现

**KING LOONG案例遗漏**:
- 装率档位（DWT验证）: 0/5方识别
- Master Remark法律效力: 0/5方识别

**调优建议**:
- 在brief的context中注入checklist
- 引入"覆盖率指标": 每个checklist项至少被3/5方引用
- 支持多模型家族: 允许不同party使用不同模型族

### 3.2 残差严重度的合理性

**当前机制**:
```json
"severity": { "enum": ["LOW", "MEDIUM", "HIGH", "CRITICAL", "P0"] }
```
五档，无量化标准，party自行判断。

**问题**:

1. **无金额阈值**: $8,163和$1,081都是MEDIUM，但量级差8倍
2. **无组合评估**: 三个MEDIUM合计~$10,450，占总滞期费~9%
3. **disposition和closure_state语义重叠**: `unresolved` vs `open`，`escalated` vs `blocked`边界不清

**调优建议**:
- 在party prompt中注入量化指引
- 在result.json合并阶段引入组合影响评估
- 明确disposition vs closure_state的语义分工: disposition是"真相状态"，closure_state是"流程状态"

### 3.3 残差类型的覆盖度

**当前机制**:
```json
"residual_type": { "type": "string", "minLength": 1, "maxLength": 100 }
```
完全自由文本，无枚举约束。

**问题**:

1. **无标准类型表**: 不同party可能用不同字符串描述同一问题
2. **类型无法驱动后续行为**: 系统无法根据类型自动选择closure策略
3. **缺失的类型**: `operational-ambiguity`, `evidence-quality`, `precedent-conflict`, `regulatory-gap`

**调优建议**:
保持自由文本，但在brief的context中注入推荐类型表:
```
推荐residual_type:
- contract-ambiguity: 合同条款歧义
- calculation-ambiguity: 计算方法/单位歧义
- documentation-gap: 证据/文档缺失
- operational-ambiguity: 码头/港口操作分类争议
- evidence-quality: 证据质量/可靠性问题
- precedent-conflict: 先例与当前案例冲突
- regulatory-gap: 法规/规则适用性缺口
```

### 3.4 残差closure的机制

**当前机制**:
schema完备: `closure_state: open|closed|blocked|waived|not_applicable`，有`required_closure`和`closure_evidence`。

但`run.rs`中没有任何更新closure_state的逻辑。残差写入result.json后即静态。

**问题**:

1. **无CLI命令**: 没有`quinte residual`子命令
2. **无closure证据验证**: `closure_evidence`是任意字符串数组，没有验证是否指向真实文件
3. **无closure工作流**: QUINTE只负责识别和结构化残差，不负责关闭
4. **无closure时效**: 没有超时机制

**调优建议**:

扩展CLI:
```bash
quinte residual list <run-id> --json
quinte residual show <run-id> <id> --json
quinte residual update <run-id> <id> --state closed --evidence proof.pdf --json
quinte residual escalate <run-id> <id> --reason "超时未关闭" --json
```

引入closure自动化层:
- `documentation-gap`: 自动检查evidence_roots是否补充了缺失文件
- `calculation-ambiguity`: 用不同参数重算验证

引入closure时效:
```json
{
  "residual_closure_ttl": {
    "HIGH": "7d",
    "MEDIUM": "14d",
    "LOW": "30d"
  }
}
```

---

## 四、QUINTE现状评估

### 做得好的地方

1. **状态机设计严谨**: 原子写入、事件日志、崩溃恢复
2. **证据快照不可变**: 防止分析过程中证据被修改
3. **R2匿名化消偏**: 隐藏route身份，减少品牌偏见
4. **hm握手防篡改**: challenge-nonce-digest绑定，防止重放和篡改
5. **适配器隔离**: 独立HOME/cwd/environment，禁止subagent/shell/web/write

### 做得不够的地方

1. **残差是静态的**: 写入result.json后无更新机制
2. **严重度无量化**: 5档但无标准，party自行判断
3. **residual_type自由文本**: 无枚举约束
4. **合并逻辑简单**: 保守策略，无加权投票或置信度聚合
5. **无残差追踪CLI**: inspect只返回原始result.json

### 优先级排序

| 方向 | 难度 | 价值 | 优先级 |
|------|------|------|--------|
| 残差生命周期CLI | 中 | 高 | P0 |
| 严重度量化 | 低 | 高 | P0 |
| closure自动化 | 中 | 中 | P1 |
| residual_type枚举 | 低 | 中 | P1 |
| 残差聚合分析 | 低 | 中 | P1 |
| 跨run追踪 | 高 | 低 | P2 |

---

## 五、工程调用方式

```bash
# 1. 初始化
quinte init --json

# 2. 检查环境
quinte doctor --json

# 3. 写brief
cat > brief.json << 'EOF'
{
  "brief_version": "0.1.1",
  "question": "问题",
  "context": "上下文+业务规则+checklist",
  "evidence_roots": ["/tmp/evidence"],
  "attachments": [],
  "action_scope": "决策范围"
}
EOF

# 4. 运行（后台模式）
quinte run --brief brief.json --json
# 返回 run_id 和 status: queued

# 5. 等待完成
quinte wait <run-id> --json
# 状态变化: queued → r1_running → r2_running → r3_cc → waiting_hm

# 6. hm握手（当status=waiting_hm时）
quinte hm request <run-id> --json
# 读取 r3/evidence-packet.json 和 r3/cc-response.json
# 独立起草 ArbiterVerdict
quinte hm submit <run-id> --verdict verdict.json --json

# 7. 查看结果
quinte inspect <run-id> --json
# 返回 result.json + report.md
```

### 关键文件路径

```
~/.quinte/
  policy.json
  runs/<run-id>/
    manifest.json
    events.jsonl
    input/
      brief.json
      policy.json
      snapshot-manifest.json
      snapshot/root-*/...
    lanes/
      R1/<route-id>/accepted.json
      R2/<route-id>/accepted.json
    r3/
      evidence-packet.json
      cc-response.json
      input-receipt.json
      hm-request.json
      hm-response.json
    result.json
    report.md
```

---

## 六、给codex的具体任务建议

### P0: 残差生命周期CLI

1. 在`src/cli.rs`中添加`Residual`子命令
2. 在`src/store.rs`中添加残差状态持久化
3. 在`src/run.rs`中添加残差更新事件日志
4. 实现`quinte residual list/show/update/escalate`

### P0: 严重度量化

1. 在`src/model.rs`的`Policy`中添加`severity_thresholds`字段
2. 在R2/R3的party prompt中注入量化指引
3. 在合并阶段添加组合影响评估逻辑

### P1: closure自动化

1. 在`src/run.rs`中添加`auto_closure_check`函数
2. 对`documentation-gap`类型，检查snapshot中是否补充了缺失文件
3. 对`calculation-ambiguity`类型，用不同参数重算验证

### P1: residual_type推荐

1. 在`Brief`中添加`recommended_residual_types`字段
2. 在party prompt中注入推荐类型表
3. 在合并阶段统计类型分布

---

**文件位置**: `/Users/ericstone/Downloads/QUINTE残差管理机制调优分析.md`
**用途**: 供codex参考，用于QUINTE残差机制的设计和实现
