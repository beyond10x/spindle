---
format: aep.planning-md/3
id: story:rally-example-reads-stdin
kind: story
status: draft
title: The rally example's stdin run reads its lines
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

The "Drive it by hand" example of `website/docs/examples/rally.md` runs as written: the command it
gives reads JSON lines on stdin and answers them. Today `examples/rally/ess` declares `rally-table`
with `reached_by: network`, so `run` on the laptop environment serves HTTP and never reads stdin; a
probe with `timeout 20` exited 124 after printing only the listening line.

Found by the wave 27 U1 implementor and adversary; the same on the base c61a59650, so pre-existing.

## Acceptance

- `the_rally_example_reads_stdin_as_its_page_says`: the page's command, run on the example with the
  page's input lines, prints one answer per line and exits 0.
