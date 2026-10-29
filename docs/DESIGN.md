# UI design system

The interface borrows from the copy-editor's desk: paper, ink, a blue pencil for the researcher's own actions, and proof-correction red reserved for deadlines. It's a tool for reading editorial correspondence quickly and deciding what to save, so legibility comes before decoration.

## Principles

1. **Show the evidence.** Every proposal leads with the words from the editor's letter that the parser relied on, set in the serif face. This is the one bold element. Everything around it stays quiet.
2. **The margin mark means "needs your decision."** Review items carry a blue-pencil bar in the left margin. Nothing else uses it.
3. **Red means a date.** Proof red appears only on deadlines that are close or overdue, never on decoration or general errors' borders.
4. **Never color alone.** Every status has a word and an icon; confidence is written out ("clear", "probable", "uncertain").
5. **Buttons say what happens.** "Add to JSS-2026-0142", "Start tracking", "Save and add", not "Confirm" or "Submit".
6. **Quiet chrome.** No shadows, gradients, all-caps labels, middle-dot meta strings, or monospace for data. IDs use tabular figures.

## Color tokens

Defined in `ui/src/styles.css` (`:root`, with a dark variant under `prefers-color-scheme: dark`). Components use the semantic names, never raw hex.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--paper` | `#F5F6F3` | `#141A23` | Window background |
| `--sheet` | `#FFFFFF` | `#1B2330` | Review items, open timeline |
| `--ink` | `#18212F` | `#E6EAF0` | Body text |
| `--margin` | `#5C6573` | `#A3ACB9` | Secondary text |
| `--rule` | `#DCE0E6` | `#2E3846` | Borders, timeline line |
| `--pencil` | `#2B57C0` | `#8FAAF2` | Primary actions, margin bar, focus ring |
| `--pencil-tint` | `#E8EEFB` | `#22304A` | Selected navigation |
| `--proof` | `#B3261E` | `#F2958C` | Close or overdue deadlines |
| `--positive` | `#2F7A4C` | `#86CFA0` | Accepted |
| `--caution` | `#965A06` | `#E9B565` | Revision required, uncertainty notes |

All text pairs meet WCAG AA (4.5:1) on their backgrounds in both themes.

## Type

- **Atkinson Hyperlegible** (400, 700): all interface text. Base 16px, line height 1.5.
- **Crimson Pro** (400, 500): only for words quoted from mail (the evidence line) and the empty-state headlines. Line height 1.35.
- Scale (ratio 1.25): 13, 16, 20, 25, 31 px. Body text never below 13px.
- Both fonts are bundled through `@fontsource` (OFL-1.1). The app must not load fonts or anything else from the network.

## Layout

```
┌─────────────┬──────────────────────────────────────────┐
│ Manuscript  │  Review                                  │
│ Hub         │                                          │
│             │  ┃ “major revision”                      │
│ Review    3 │  ┃ Journal   Journal of Synthetic Studies │
│ Manuscripts │  ┃ Due       12 November 2026, in 14 days │
│           2 │  ┃ [Add to JSS-2026-0142] [Edit] Ignore   │
│             │                                          │
│ Scan demo   │  ┃ next item                             │
│ mailbox     │                                          │
└─────────────┴──────────────────────────────────────────┘
```

- A left rail holds navigation and the scan action. Under 760px wide it becomes a top bar.
- The main column is left-aligned, with a maximum line length of about 72 characters.
- Manuscripts are a ledger of rows (status, journal and ID, next deadline). A row expands in place to show its timeline, a real sequence, so it uses a vertical line with dated steps.

## Interaction

- Visible `:focus-visible` ring (2px pencil, 2px offset) on every control. Targets are at least 40px tall.
- Motion only answers an action: the timeline opening and hover feedback. Transitions are 160ms and are disabled under `prefers-reduced-motion`.
- Results of an action appear next to it (`role="status"`); errors use `role="alert"` and say what to do next.

## Previewing without the desktop shell

`npm run dev` serves the UI in a browser with an in-memory mock backend (`ui/src/lib/mock.ts`, dev builds only), so you can work on the design without Rust. Open `#manuscripts` to start on the ledger.
