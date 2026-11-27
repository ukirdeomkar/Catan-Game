# Bot Learning Roadmap (exploratory)

Status: **draft / not started.** This document captures a spike into "can we make
the bots play better with a learned model?" — what we tried, what we learned, and
the staged plan if we ever want to pursue it. Nothing here is implemented.

The current bots (`src/bot.rs`) are a hand-tuned, deterministic heuristic with
three tiers (Easy / Medium / Hard). This doc is about the *alternative*: training
a model to play, and the self-play path that could actually beat the heuristic.

---

## TL;DR

- We evaluated **Laya** (a small non-autoregressive "System One" decision model,
  `pip install laya`, ModernBERT-large + decision head) as a drop-in bot brain.
- **Prompting it does not work.** It has no Catan knowledge and cannot do the
  trivial arithmetic the heuristic already does. More prompt did not help; its
  confidence collapsed to ~0 because the input is out of distribution.
- A model trained on the current bot's moves only **imitates** the heuristic
  (ties at best). To play *better* you need **self-play RL** (learn from
  win/loss), which is a real project.
- If we pursue learning, the right tool is a **small feature-based net + self-play
  + a tournament harness**, *not* an 800 MB text encoder.
- The **cheapest win** remains improving the Rust heuristic directly (1–2 ply
  lookahead for the Hard tier).

---

## 1. What we tried: Laya

### What Laya is
- `laya` (PyPI 0.3.28, `convaiinnovations/laya` on HuggingFace), described as a
  "fast, non-autoregressive System 1 decision engine with calibrated
  probabilities."
- Architecture (from the shipped `rl_agent_config.json`): **ModernBERT-large
  encoder** (~395M params, ~800 MB weights) + a 2-layer decision head. It was
  fine-tuned (~7,313 updates, ~2 h) as an "rl-agent."
- API: `Router.predict(state, questions)` where `questions` can be `choice`
  (pick one of `criteria`) / `score` / `noul`. Returns a probability per option.
- It is **not** an agent: no game tree, no rules, no memory. Text in → one label
  out, single forward pass.

### How we ran it locally
- Installed into a throwaway venv (kept out of the repo):
  `%TEMP%\opencode\laya-venv`.
- Weights fetched to the HF cache (~807 MB) and loaded from a **local folder** to
  avoid Hub round-trips:
  ```python
  from laya import Router
  SNAP = r"...\models--convaiinnovations--laya\snapshots\<rev>"
  router = Router(models={"english": SNAP})   # overrides ONLY english
  router.preload(["english"])                  # do NOT preload all checkpoints
  res = router.predict(state, {"action": {"type": "choice", "instructions": ..., "criteria": {...}}})
  ```
- Load ~6 s on CPU; ~0.4–0.8 s per decision on CPU.

### Findings (the important part)
- **In-domain works:** an email-routing example returned `billing` at 0.98
  probability — sharp.
- **Catan does not work**, and a better prompt does not fix it:

  | Scenario | Prompt | Chose | Confidence | Correct? |
  |---|---|---|---|---|
  | Setup settlement | naive | D (worst: 7 pips, 2 resources) | 0.02 | no |
  | Setup settlement | expert (rules + pip table + method) | C (8 pips; B=12 is best) | **0.004** | no |
  | Robber placement | expert | hex that blocks *its own* settlement | 0.015 | no |

- The "expert" prompt (full Catan rules, the dice-pip table, an explicit scoring
  method, and per-spot pip counts) made the probabilities nearly **uniform**
  (`{A .25, B .25, C .29, D .22}`) and confidence ~0. The model is honestly
  signalling "out of distribution."
- **Why:** an encoder maps surface wording to a label in one pass. It does not do
  chain-of-thought or arithmetic. Writing "add the pips, prefer 6/8, value
  Wheat/Ore" does not teach it Catan — it's just more noise in the input. The
  arithmetic it's being asked to do is 3 lines of `pip(n)` + distinct-resource
  counting that `src/bot.rs` already does correctly and instantly.

### Conclusion on Laya
Not a viable "smarter bot" via prompting. Could only help if **fine-tuned** on
Catan decisions — but see §3 for why the text-encoder shape is the wrong tool for
that.

---

## 2. Laya can be fine-tuned (for the record)

`laya` ships a training CLI, so the path exists if we ever want it:

```
laya-train --data decisions.jsonl --out my-laya --base <laya-dir> \
           --loss soft-ce --epochs 4 [--freeze-encoder] [--device cuda]
```
- Data: JSONL/CSV of labelled decisions (state + question + chosen label).
- `--loss soft-ce` = imitation; `--loss rlcd` = preference/RL-style objective.
- `--freeze-encoder` trains only the head (CPU-viable); full fine-tune wants a GPU.
- Output is another ~800 MB checkpoint.

The toolkit is real; the constraint is the architecture and compute (below).

---

## 3. The key distinction: imitation vs. reinforcement learning

This is the crux of any "train on our current logic, then improve" idea.

- **Imitation / behaviour cloning** (train on the bot's moves):
  - Ceiling = the teacher. Ties at best, usually a bit below (can't do exact math).
  - There is **no outcome signal** in the data ("this move won, do more of it"),
    so it cannot improve past what it copies.
- **Reinforcement learning / self-play** (learn from wins and losses):
  - Ceiling = reward design + compute. **This is how AlphaZero-style systems beat
    hand-written heuristics.**
  - This is the only path that can genuinely play *better* than Hard.

So: **not by imitating the bot — yes, potentially, by learning from outcomes.**

---

## 4. Recommended approach if we pursue learning

Do **not** route board state through a text encoder. Use a **compact
feature-based policy/value net**, trained by self-play, with a measured scoreboard.

### Observation (features, not text)
- Board: per-hex terrain + number + robber, per-vertex building/owner, per-edge
  road/owner; port locations.
- Self: resources, dev cards, pieces left, public + hidden VP.
- Public: each opponent's visible pieces/VP/dev played, longest road/army holder.
- Turn/phase, dice, and **importantly not** the opponent's difficulty tier — the
  model must infer opponent strength from behaviour, like a human.

### Actions
- Choose among the current legal moves from the engine's `legal_*()` helpers
  (variable-size action set). Reuse the existing `Action` enum.

### Reward
- Primary: win = +1, loss = −1 (zero-sum, 4-player).
- Optional shaping to speed learning: VP gained, cities, Largest Army / Longest
  Road, resource denial, robber efficiency.

### Opponents (the "league")
- Start with the existing Easy / Medium / Hard bots.
- Add the learner's **own past checkpoints** and itself. Without a league the
  model overfits to the three deterministic bots and collapses
  (rock-paper-scissors / cycles).
- Optionally randomise bot seeds/behaviour to reduce determinism exposure.

### Style
- Stochastic policy (sample, or temperature) → varied play, which is also the
  "interesting for the user" goal.

---

## 5. Hardware reality on the dev machine

Measured on the development laptop:

| Resource | Value | Implication |
|---|---|---|
| GPU | RTX 3050 Ti Laptop, **4 GB VRAM** | Full fine-tune of a 395M encoder will likely OOM (~3.2 GB+ params+grads+Adam, plus activations). A small feature net fits easily. |
| CPU | i7-11800H, 8c/16t | Fine for self-play data generation and small-net training. |
| RAM | 16 GB | Fine. |
| Disk free | ~15 GB (C:) | Tight; datasets + checkpoints add up. |
| torch | CPU-only build installed | Need CUDA build for GPU training. |

**Bottom line:** self-play RL with a *small* net is feasible here; training the
800 MB text encoder is not practical.

---

## 6. Staged plan (each stage measurable; stop if it doesn't beat Hard)

### Stage 1 — Harness (no ML)
- Extract/duplicate the all-bot driver pattern from `src/bot.rs`
  (`play_out`, used by tests) into a runnable **self-play + tournament** binary
  or example.
- Self-play mode: run games between tier mixes (Easy/Medium/Hard), emit per-decision
  rows (§7) to JSONL.
- Tournament mode: given a policy (starting with the built-in tiers), play N games
  head-to-head vs each tier and print win rates + confidence intervals.
- Deliverable: `scripts/` (or a `cargo` example/bin) + docs; gives us the
  scoreboard we need before any model exists.

### Stage 2 — Compact feature net + trainer (PyTorch)
- Encode `GameState` → fixed feature vector; legal `Action`s → action features
  (+ mask). Small MLP/attention head (~1–5M params) → policy + value.
- Warm-start by **imitating Hard** (`soft-ce`-style), then **self-play RL**
  against the league (policy-gradient or AlphaZero-style with a search).
- Deliverable: training script + a checkpoint the game can query.

### Stage 3 — Integrate + measure
- Add a "Learned" bot tier in `src/bot.rs` / driver that queries the model
  (in-process via `pyo3`/ONNX, or a sidecar).
- Run the tournament vs Easy/Medium/Hard; report win rates.
- Decision gate: if it does not clear Hard, keep the heuristic.

---

## 7. Data schema (proposed, for Stage 1)

One JSON object per decision (JSONL), so it feeds either a text model or a feature
net:

```json
{
  "game_id": "uuid",
  "turn": 14,
  "player": 2,
  "phase": "Play",
  "state_text": "human-readable state (for optional text models)",
  "features": { "...": "flattened numeric observations" },
  "legal_actions": [
    {"id": "build_settlement:12", "kind": "BuildSettlement", "args": {"vertex": 12}},
    {"id": "end_turn", "kind": "EndTurn", "args": {}}
  ],
  "chosen_action": "build_settlement:12",
  "opponents": [{"player": 1, "tier": "Hard"}, {"player": 3, "tier": "Medium"}],
  "outcome": {"winner": 2, "your_final_vp": 10, "turns": 61}
}
```
- Opponent tier is stored for analysis but **must not** be an input feature.
- `outcome` is filled at game end (needed for RL; harmless for imitation).

---

## 8. Evaluation methodology

- **Head-to-head, mixed seats, many games** (e.g. ≥ 1,000 per matchup) with the
  engine's seeded RNG; report win rate + CI, not anecdotes.
- Seat-balanced (model plays each of the 4 seats equally).
- Report vs Easy, vs Medium, vs Hard, and vs a mixed pool.
- Track: win rate, average final VP, game length, and "does it vary its opening"
  (style/diversity).

---

## 9. Risks / open questions

- **Payoff uncertainty for Catan:** imperfect information, 4-player, stochastic
  dice → RL is harder than chess/Go; AlphaZero-style may need a lot of games.
- **Determinism of teachers:** training only against deterministic bots invites
  exploitation; mitigate with a league + randomised seeds.
- **Latency:** a net adds per-turn cost vs the current instant heuristic (Laya was
  ~0.5 s on CPU; a small net would be ≪ that, especially in-process/ONNX).
- **Maintenance:** a learned tier is a new artifact to version, host, and keep in
  sync with the engine's `Action` space.
- **Is it worth it vs. lookahead?** A 1–2 ply lookahead in Rust likely beats both
  current Hard and an imitated model at a fraction of the cost.

---

## 10. Alternatives considered

1. **Rust lookahead (recommended, cheapest):** add 1–2 ply search to the Hard
   tier (branch over legal actions, evaluate resulting `GameState`). No ML, no
   latency, no 800 MB dependency; likely the biggest immediate strength gain.
2. **Prompt an LLM/Laya at runtime:** rejected — see §1.
3. **Imitate the heuristic only:** rejected as a *strength* play; it can't exceed
   the teacher.
4. **Fine-tune the 800 MB text encoder:** possible (`laya-train`) but wrong shape
   and too heavy for this hardware; prefer a feature net.

---

## 11. Appendix: reproduce the Laya spike

```powershell
# isolated venv (outside the repo)
py -3.12 -m venv %TEMP%\opencode\laya-venv
%TEMP%\opencode\laya-venv\Scripts\python -m pip install laya

# fetch weights once (needs a network where huggingface.co is reachable)
%TEMP%\opencode\laya-venv\Scripts\python -c "from huggingface_hub import snapshot_download; snapshot_download('convaiinnovations/laya', allow_patterns=['config.json','encoder/*','model.safetensors','rl_agent_config.json','tokenizer/*'])"
```

Notes / gotchas hit during the spike:
- Offline loading: point `Router(models={"english": <local snapshot dir>})`; do
  **not** `preload=True` (it tries to load `multilingual`/`typed-decisions` too and
  will hit the Hub).
- `HF_HUB_OFFLINE=1` + a *pinned* revision mismatch (`main` vs pinned commit)
  causes `IncompleteSnapshotError`; either fetch the default revision or load from
  a local path.
- The shipped checkpoint logs a warning that its temperature calibration is
  invalid; treat confidence as uncalibrated.
- No HF token needed for this public model.

References:
- Laya: https://huggingface.co/convaiinnovations/laya · PyPI `laya`
- Article context: DataCamp "Top 7 Open-Source TypeSafe Jev Alternatives"
