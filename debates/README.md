# QUINTE Debate Logs

Each subdirectory is one complete QUINTE debate session. Naming: `YYYY-MM-DD-<topic-slug>/`.

## Structure

```
debates/
├── 2026-06-09-v3-ratification/     # cc 主导的 v3.0 协议五方通过辩论
│   ├── claude_r1.md                # cc R1 独立分析
│   ├── claude_r2.md                # cc R2 交叉评审
│   ├── codewhale_r1.md             # cw R1 深度分析
│   ├── codewhale_r2.md             # cw R2 交叉评审
│   ├── omp_r1.md                   # omp R1 快速推理
│   ├── omp_r2.md                   # omp R2 交叉评审
│   ├── r1_summary.md               # R1 汇总
│   └── final_verdict.md            # 终裁
│
└── 2026-06-09-v3-self-audit/       # hm 主导的 v3.0 自审 (新架构首次 QUINTE)
    ├── cc_q1.md                    # cc 审计 PROTOCOL vs GATES 證門矛盾
    ├── cc_q234.md                  # cc 审计 loop/跨模型/KANSA
    ├── cw_r1.md                    # cw R1 深度审计 (6维 mismatch 表)
    ├── omp_r1.md                   # omp R1 (Linux 平替版, 无文件读取)
    ├── rx_r2.md                    # rx R2 纯推理交叉裁判
    ├── v3_requinte_final.md        # hm 终裁报告
    └── MAC_REQUINTE_MEMO.md        # Mac 全量 QUINTE 重跑备忘
```

## Notes

- `omp_r1.md` 文件可能包含 `=== REASONING ===` 段 — omp 的推理过程
- cc/cw 的 thinking/reasoning 为内部过程, 不在此日志中
- 命名规则: `{agent}_{round}.md` 或 `{agent}_q{N}.md` (分题审计)
