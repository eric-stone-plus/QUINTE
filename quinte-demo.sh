1|#!/usr/bin/env bash
2|# quinte-demo.sh — Simulate a QUINTE debate round
3|# Demonstrates the architecture: R1 parallel → R2 cross-review → R3 verdict
4|set -e
5|
6|echo "╔══════════════════════════════════════════╗"
7|echo "║         QUINTE DEBATE DEMO              ║"
8|echo "║  Hermes + cc + cw + Reasonix + OMP      ║"
9|echo "╚══════════════════════════════════════════╝"
10|echo ""
11|
12|PROMPT="${1:-What is the most reliable approach to Chrome browser automation for AI agents?}"
13|OUTDIR="${2:-/tmp/quinte-demo}"
14|mkdir -p "$OUTDIR"
15|
16|# ── Round 1: Independent Analysis ──
17|echo "═══ Round 1: Independent Analysis ═══"
18|echo "→ Launching Hermes + Claude Code + CodeWhale in parallel..."
19|echo ""
20|
21|# Hermes
22|echo "[Hermes] analyzing..."
23|cat > "$OUTDIR/hermes_r1.md" << 'EOF'
24|# Hermes R1 Analysis
25|
26|## Root Cause
27|Chrome CDP + AI agent (OMP) is architecturally mismatched. The AI can click any element.
28|
29|## Recommendation
30|Pure Python CDP scripts with hardcoded selectors. Never let an AI agent guess where to click.
31|EOF
32|echo "  ✓ hermes_r1.md"
33|
34|# Claude Code (simulated)
35|echo "[Claude Code] analyzing..."
36|cat > "$OUTDIR/claude_r1.md" << 'EOF'
37|# Claude Code R1 Analysis
38|
39|## Option A (Pure Python CDP) is the clear winner.
40|
41|- Target.createTarget guarantees new tabs
42|- Input.dispatchKeyEvent bypasses React button issues
43|- asyncio.gather for clean parallelism
44|- ~550 lines estimated
45|
46|## Rejected alternatives
47|- OMP prompts: can't constrain an unbounded tool
48|- AppleScript: synchronous, can't parallelize
49|EOF
50|echo "  ✓ claude_r1.md"
51|
52|# CodeWhale
53|echo "[CodeWhale] researching..."
54|cat > "$OUTDIR/codewhale_r1.md" << 'EOF'
55|# CodeWhale R1 Analysis
56|
57|## Implementation: Playwright CDP
58|Built 635-line script with BrowserContext isolation, explicit selectors,
59|wait-and-verify pattern, timeout-bounded operations.
60|
61|## Key findings
62|- OMP browser tool is the wrong layer — it's an AI agent driving a browser
63|- CDP scripts are deterministic — no AI guessing
64|- Playwright adds 300MB overhead vs raw WebSocket CDP
65|EOF
66|echo "  ✓ codewhale_r1.md"
67|
68|echo ""
69|echo "  [cc status: OK, no timeout needed. OMP stays as spare]"
70|echo ""
71|
72|# ── Round 2: Cross-Review ──
73|echo "═══ Round 2: Cross-Review ═══"
74|echo "→ All 5 agents review each other's R1 outputs..."
75|echo ""
76|
77|echo "[Reasonix] Cross-examining..."
78|cat > "$OUTDIR/reasonix_r2.md" << 'EOF'
79|# Reasonix R2 Cross-Review
80|
81|## 3/3 Consensus
82|1. CDP Python scripts over OMP browser
83|2. Target.createTarget for tab isolation
84|3. Hardcoded selectors over AI guessing
85|
86|## Disagreement
87|- cc/cw: Playwright CDP
88|- Hermes/OMP: Raw WebSocket CDP (lighter)
89|
90|## OMP role
91|Strip OMP from automation layer. Keep as reasoning engine for result synthesis.
92|EOF
93|echo "  ✓ reasonix_r2.md"
94|
95|for agent in hermes claude codewhale OMP; do
96|  if [ "$agent" = "OMP" ]; then
97|    echo "[OMP] Cross-reviewing (spare activated for R2)..."
98|  else
99|    echo "[${agent}] Cross-reviewing..."
100|  fi
101|  cat > "$OUTDIR/${agent}_r2.md" << EOF
102|# ${agent} R2 Cross-Review
103|
104|Reviewed: all other agents' R1 outputs.
105|Consensus: CDP scripts > OMP browser.
106|Unique catch: ${agent}-specific finding placeholder.
107|EOF
108|  echo "  ✓ ${agent}_r2.md"
109|done
110|
111|echo ""
112|
113|# ── Round 3: Final Verdict ──
114|echo "═══ Round 3: Final Verdict ═══"
115|echo "→ Hermes synthesizing all findings..."
116|echo ""
117|
118|cat > "$OUTDIR/final_verdict.md" << 'EOF'
119|# Final Verdict: Browser Automation Approach
120|
121|**5-party consensus** (Hermes + cc + cw + Reasonix + OMP):
122|
123|## Decision
124|Raw WebSocket CDP scripts — lighter than Playwright, more precise than OMP browser.
125|
126|## Architecture
127|1. cdp_session.py (~150 lines) — WebSocket session manager
128|2. selectors.py (~80 lines) — aria→CSS→text fallback chains
129|3. page_actions.py (~100 lines) — Runtime.evaluate with verification
130|
131|## OMP role
132|Removed from browser automation. Retained as:
133|- Grok/Gemini reply comparison & synthesis
134|- LSP/DAP debugging (unique capability)
135|- QUINTE debate participant (hot spare + R2 reviewer)
136|EOF
137|echo "  ✓ final_verdict.md"
138|
139|echo ""
140|echo "╔══════════════════════════════════════════╗"
141|echo "║         DEBATE COMPLETE                 ║"
142|echo "║  5 agents · 3 rounds · 1 verdict        ║"
143|echo "║  Output: $OUTDIR                         ║"
144|echo "╚══════════════════════════════════════════╝"
145|echo ""
146|echo "Files produced:"
147|ls -la "$OUTDIR"/*.md
148|