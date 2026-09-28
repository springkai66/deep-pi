# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this table.

Edit the right-hand column to match whatever vocabulary you actually use.

## Wayfinder labels

`/wayfinder` reads and writes a separate label group, `<skill>:<name>`, on the map and its child tickets. All five exist in this repo's tracker.

| Label               | Applied to                                        |
| ------------------- | ------------------------------------------------- |
| `wayfinder:map`     | The map itself, the canonical artifact            |
| `wayfinder:research`| A ticket answered by reading primary sources      |
| `wayfinder:prototype`| A ticket answered by building a throwaway spike  |
| `wayfinder:grilling`| A ticket answered by grilling the user            |
| `wayfinder:task`    | A ticket answered by just doing the work          |

Everything else in the tracker (`bug`, `enhancement`, `documentation`, …) is free vocabulary: apply it when it genuinely fits, never force it.
