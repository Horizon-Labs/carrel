# Restructure benchmark report

2026-11-02 on macos aarch64, carrel 0.6.0: a whole-archive proposal for the muddled test archive at each eagerness, with numbers alone and with each text model.

## Summary

| Wording | Eagerness | Changes | Shape before → after | Fixed | Broken | Moves (weighted) | Findability gain | s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| numbers alone | settled | 2 | 77% → 79% | 2 of 18 | 0 | 2 (8) | untested | 0.0 |
| numbers alone | balanced | 2 | 77% → 79% | 2 of 18 | 0 | 2 (8) | untested | 0.0 |
| numbers alone | eager | 2 | 77% → 79% | 2 of 18 | 0 | 2 (8) | untested | 0.0 |
| apple/Apple Foundation Model | settled | 0 | 77% → 77% | 0 of 18 | 0 | 0 (0) | +0 points | 61.6 |
| apple/Apple Foundation Model | balanced | 0 | 77% → 77% | 0 of 18 | 0 | 0 (0) | +0 points | 48.3 |
| apple/Apple Foundation Model | eager | 2 | 77% → 79% | 2 of 18 | 0 | 2 (8) | +5 points | 48.9 |
| llama/granite-4.2-3b.gguf | settled | 0 | 77% → 77% | 0 of 18 | 0 | 0 (0) | +0 points | 49.3 |
| llama/granite-4.2-3b.gguf | balanced | 0 | 77% → 77% | 0 of 18 | 0 | 0 (0) | +0 points | 50.1 |
| llama/granite-4.2-3b.gguf | eager | 0 | 77% → 77% | 0 of 18 | 0 | 0 (0) | +0 points | 50.4 |
| llama/qwen3.5-4b.gguf | settled | 2 | 77% → 79% | 2 of 18 | 0 | 2 (8) | +2 points | 60.6 |
| llama/qwen3.5-4b.gguf | balanced | 2 | 77% → 79% | 2 of 18 | 0 | 2 (8) | +2 points | 59.6 |
| llama/qwen3.5-4b.gguf | eager | 2 | 77% → 79% | 2 of 18 | 0 | 2 (8) | +2 points | 59.5 |

**Shape** is how closely the items that end up together match the intended shape (the pairs' F1), before and after the proposal; it recovers the shape when it rises. **Fixed** is the misplaced items it puts in an acceptable collection or one inside it, and **broken** the well-placed items it moves out of one. **Moves** are items moved, and weighted by each change's invasiveness (1 for rewriting to 5 for top-level collections). **Findability gain** is its own test's held-out cards found in the proposed tree less those found in the current one; numbers alone can't run it.

Items were grouped by their cards' embeddinggemma-300m vectors and the tag and link graph.

The crawler's model here, llama/qwen3.5-4b.gguf, has no run in the crawler benchmark (/Users/carrel-ci/bench/out-restr5/crawl/runs/llama--qwen3.5-4b.jsonl), so proposals had no crawler verdicts.

## numbers alone, settled

- Change: move to Receipts: receipt desk chair.txt
- Change: move to Insurance: car insurance renewal.txt
- moving items between branches: 2 items
- 4.0 weighted moves for each item it fixes
- Untested: no text model was at hand to run the findability test
- Held back: a new collection of 4 items in Home: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 4 items in Projects: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Home: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Projects: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new top-level collection of 15 items: its gain (-0.30) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 8 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.36) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 13 items: its gain (-0.38) isn't the 0.45 moving or merging top-level collections needs at this eagerness

## numbers alone, balanced

- Change: move to Receipts: receipt desk chair.txt
- Change: move to Insurance: car insurance renewal.txt
- moving items between branches: 2 items
- 4.0 weighted moves for each item it fixes
- Untested: no text model was at hand to run the findability test
- Held back: a new collection of 4 items in Home: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 4 items in Projects: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Home: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Projects: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new top-level collection of 15 items: its gain (-0.30) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 8 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.36) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 13 items: its gain (-0.38) isn't the 0.22 moving or merging top-level collections needs at this eagerness

## numbers alone, eager

- Change: move to Receipts: receipt desk chair.txt
- Change: move to Insurance: car insurance renewal.txt
- moving items between branches: 2 items
- 4.0 weighted moves for each item it fixes
- Untested: no text model was at hand to run the findability test
- Held back: a new collection of 4 items in Home: it costs as much as it gains
- Held back: a new collection of 4 items in Projects: it costs as much as it gains
- Held back: a new collection of 5 items in Home: it costs as much as it gains
- Held back: a new collection of 5 items in Projects: it costs as much as it gains
- Held back: a new top-level collection of 15 items: it costs as much as it gains
- Held back: a new top-level collection of 8 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 13 items: it costs as much as it gains

## apple/Apple Foundation Model, settled

No changes proposed.
- Findability: 26 of 40 found now, 26 after
- Held back: a new collection of 4 items in Home: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 4 items in Projects: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Home: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Projects: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new top-level collection of 15 items: its gain (-0.30) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 8 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.36) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 13 items: its gain (-0.38) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: moving an item from Reading into Receipts: the new shape wasn't easier to find things in, as tested
- Held back: moving an item from Travel into Insurance: the new shape wasn't easier to find things in, as tested

## apple/Apple Foundation Model, balanced

No changes proposed.
- Findability: 26 of 40 found now, 26 after
- Held back: a new collection of 4 items in Home: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 4 items in Projects: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Home: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Projects: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new top-level collection of 15 items: its gain (-0.30) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 8 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.36) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 13 items: its gain (-0.38) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: moving an item from Reading into Receipts: the new shape wasn't easier to find things in, as tested
- Held back: moving an item from Travel into Insurance: the new shape wasn't easier to find things in, as tested

## apple/Apple Foundation Model, eager

- Change: move to Receipts: receipt desk chair.txt
- Change: move to Insurance: car insurance renewal.txt
- moving items between branches: 2 items
- 4.0 weighted moves for each item it fixes
- Findability: 25 of 40 found now, 27 after
- Held back: a new collection of 4 items in Home: it costs as much as it gains
- Held back: a new collection of 4 items in Projects: it costs as much as it gains
- Held back: a new collection of 5 items in Home: it costs as much as it gains
- Held back: a new collection of 5 items in Projects: it costs as much as it gains
- Held back: a new top-level collection of 15 items: it costs as much as it gains
- Held back: a new top-level collection of 8 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 13 items: it costs as much as it gains

## llama/granite-4.2-3b.gguf, settled

No changes proposed.
- Findability: 26 of 40 found now, 26 after
- Held back: a new collection of 4 items in Home: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 4 items in Projects: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Home: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Projects: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new top-level collection of 15 items: its gain (-0.30) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 8 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.36) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 13 items: its gain (-0.38) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: moving an item from Reading into Receipts: the new shape wasn't easier to find things in, as tested
- Held back: moving an item from Travel into Insurance: the new shape wasn't easier to find things in, as tested

## llama/granite-4.2-3b.gguf, balanced

No changes proposed.
- Findability: 26 of 40 found now, 26 after
- Held back: a new collection of 4 items in Home: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 4 items in Projects: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Home: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Projects: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new top-level collection of 15 items: its gain (-0.30) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 8 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.36) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 13 items: its gain (-0.38) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: moving an item from Reading into Receipts: the new shape wasn't easier to find things in, as tested
- Held back: moving an item from Travel into Insurance: the new shape wasn't easier to find things in, as tested

## llama/granite-4.2-3b.gguf, eager

No changes proposed.
- Findability: 26 of 40 found now, 26 after
- Held back: a new collection of 4 items in Home: it costs as much as it gains
- Held back: a new collection of 4 items in Projects: it costs as much as it gains
- Held back: a new collection of 5 items in Home: it costs as much as it gains
- Held back: a new collection of 5 items in Projects: it costs as much as it gains
- Held back: a new top-level collection of 15 items: it costs as much as it gains
- Held back: a new top-level collection of 8 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 13 items: it costs as much as it gains
- Held back: moving an item from Reading into Receipts: the new shape wasn't easier to find things in, as tested
- Held back: moving an item from Travel into Insurance: the new shape wasn't easier to find things in, as tested

## llama/qwen3.5-4b.gguf, settled

- Change: move to Receipts: receipt desk chair.txt
- Change: move to Insurance: car insurance renewal.txt
- moving items between branches: 2 items
- 4.0 weighted moves for each item it fixes
- Findability: 26 of 40 found now, 27 after
- Held back: a new collection of 4 items in Home: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 4 items in Projects: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Home: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Projects: its gain (-0.05) isn't the 0.15 subdividing a collection needs at this eagerness
- Held back: a new top-level collection of 15 items: its gain (-0.30) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 8 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.36) isn't the 0.45 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 13 items: its gain (-0.38) isn't the 0.45 moving or merging top-level collections needs at this eagerness

## llama/qwen3.5-4b.gguf, balanced

- Change: move to Receipts: receipt desk chair.txt
- Change: move to Insurance: car insurance renewal.txt
- moving items between branches: 2 items
- 4.0 weighted moves for each item it fixes
- Findability: 26 of 40 found now, 27 after
- Held back: a new collection of 4 items in Home: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 4 items in Projects: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Home: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new collection of 5 items in Projects: its gain (-0.05) isn't the 0.07 subdividing a collection needs at this eagerness
- Held back: a new top-level collection of 15 items: its gain (-0.30) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 8 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.35) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 4 items: its gain (-0.36) isn't the 0.22 moving or merging top-level collections needs at this eagerness
- Held back: a new top-level collection of 13 items: its gain (-0.38) isn't the 0.22 moving or merging top-level collections needs at this eagerness

## llama/qwen3.5-4b.gguf, eager

- Change: move to Receipts: receipt desk chair.txt
- Change: move to Insurance: car insurance renewal.txt
- moving items between branches: 2 items
- 4.0 weighted moves for each item it fixes
- Findability: 26 of 40 found now, 27 after
- Held back: a new collection of 4 items in Home: it costs as much as it gains
- Held back: a new collection of 4 items in Projects: it costs as much as it gains
- Held back: a new collection of 5 items in Home: it costs as much as it gains
- Held back: a new collection of 5 items in Projects: it costs as much as it gains
- Held back: a new top-level collection of 15 items: it costs as much as it gains
- Held back: a new top-level collection of 8 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 4 items: it costs as much as it gains
- Held back: a new top-level collection of 13 items: it costs as much as it gains
