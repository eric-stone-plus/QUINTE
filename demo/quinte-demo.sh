#!/usr/bin/env bash
# quinte-demo.sh — Simulate a QUINTE debate round
# Demonstrates the architecture: R1 parallel → R2 cross-review → R3 verdict
set -e

echo "╔══════════════════════════════════════════╗"
echo "║         QUINTE DEBATE DEMO              ║"
echo "║  R1: hm + cc + cw + omp (4 agents)     ║"
echo "║  R2: + Reasonix (5 agents)             ║"
echo "╚══════════════════════════════════════════╝"
echo ""

PRompT="${1:-What is the most reliable approach to Chrome browser automation for AI agents?}"
OUTDIR="${2:-/tmp/quinte-demo}"
mkdir -p "$OUTDIR"

# ── Round 1: Independent Analysis ──
echo "═══ Round 1: Independent Analysis ═══"
echo "→ Launching Hermes + Claude Code + CodeWhale + omp in parallel..."
echo ""
echo "  [Reasonix joins in R2 — run mode, pure reasoning]"

# Hermes
echo "[Hermes] analyzing..."
cat > "$OUTDIR/hermes_r1.md" << 'EOF'
# Hermes R1 Analysis

## Root Cause
Chrome CDP + AI agent (omp) is architecturally mismatched. The AI can click any element.

## Recommendation
Pure Python CDP scripts with hardcoded selectors. Never let an AI agent guess where to click.
EOF
echo "  ✓ hermes_r1.md"

# Claude Code (simulated)
echo "[Claude Code] analyzing..."
cat > "$OUTDIR/claude_r1.md" << 'EOF'
# Claude Code R1 Analysis

## Option A (Pure Python CDP) is the clear winner.

- Target.createTarget guarantees new tabs
- Input.dispatchKeyEvent bypasses React button issues
- asyncio.gather for clean parallelism
- ~550 lines estimated

## Rejected alternatives
- omp prompts: can't constrain an unbounded tool
- AppleScript: synchronous, can't parallelize
EOF
echo "  ✓ claude_r1.md"

# CodeWhale
echo "[CodeWhale] researching..."
cat > "$OUTDIR/codewhale_r1.md" << 'EOF'
# CodeWhale R1 Analysis

## Implementation: Playwright CDP
Built 635-line script with BrowserContext isolation, explicit selectors,
wait-and-verify pattern, timeout-bounded operations.

## Key findings
- omp browser tool is the wrong layer — it's an AI agent driving a browser
- CDP scripts are deterministic — no AI guessing
- Playwright adds 300MB overhead vs raw WebSocket CDP
EOF
echo "  ✓ codewhale_r1.md"

echo ""
echo "  [R1 complete: 4/4 agents delivered]"

# omp (simulated)
echo "[omp] analyzing..."
cat > "$OUTDIR/omp_r1.md" << 'EOF'
# omp R1 Analysis

## Cross-model comparison
Grok and Gemini both fail at DOM-precise clicks. Pure CDP is the only reliable path.

## Platform note
macOS: Python websocket-client, no Playwright overhead.
EOF
echo "  ✓ omp_r1.md"

echo ""

# ── Round 2: Cross-Review ──
echo "═══ Round 2: Cross-Review ═══"
echo "→ All 5 agents review each other's R1 outputs..."
echo ""

echo "[Reasonix] Cross-examining..."
cat > "$OUTDIR/reasonix_r2.md" << 'EOF'
# Reasonix R2 Cross-Review

## 3/3 Consensus
1. CDP Python scripts over omp browser
2. Target.createTarget for tab isolation
3. Hardcoded selectors over AI guessing

## Disagreement
- cc/cw: Playwright CDP
- Hermes/omp: Raw WebSocket CDP (lighter)

## omp role
Strip omp from automation layer. Keep as reasoning engine for result synthesis.
EOF
echo "  ✓ reasonix_r2.md"

for agent in hermes claude codewhale omp; do
  echo "[${agent}] Cross-reviewing..."
  cat > "$OUTDIR/${agent}_r2.md" << EOF
# ${agent} R2 Cross-Review

Reviewed: all other agents' R1 outputs.
Consensus: CDP scripts > omp browser.
Unique catch: ${agent}-specific finding placeholder.
EOF
  echo "  ✓ ${agent}_r2.md"
done

echo ""

# ── Round 3: Final Verdict ──
echo "═══ Round 3: Final Verdict ═══"
echo "→ Hermes synthesizing all findings..."
echo ""

cat > "$OUTDIR/final_verdict.md" << 'EOF'
# Final Verdict: Browser Automation Approach

**5-party consensus** (Hermes + cc + cw + Reasonix + omp):

## Decision
Raw WebSocket CDP scripts — lighter than Playwright, more precise than omp browser.

## Architecture
1. cdp_session.py (~150 lines) — WebSocket session manager
2. selectors.py (~80 lines) — aria→CSS→text fallback chains
3. page_actions.py (~100 lines) — Runtime.evaluate with verification

## omp role
Removed from browser automation. Retained as:
- Grok/Gemini reply comparison & synthesis
- LSP/DAP debugging (unique capability)
- QUINTE debate participant (hot spare + R2 reviewer)
EOF
echo "  ✓ final_verdict.md"

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║         DEBATE CompLETE                 ║"
echo "║  5 agents · 3 rounds · 1 verdict        ║"
echo "║  Output: $OUTDIR                         ║"
echo "╚══════════════════════════════════════════╝"
echo ""
echo "Files produced:"
ls -la "$OUTDIR"/*.md
