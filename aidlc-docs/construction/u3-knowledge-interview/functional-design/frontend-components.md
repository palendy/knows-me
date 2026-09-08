# U3 — Frontend Components (Interview Queue)

> Design of `InterviewQueueView` (`src/features/queue/`). Interaction model = **inline expansion** (Q11=A): the owner answers within the list, no screen navigation (US-4.2/AC1).
>
> **Scaffolding note (Q1=A)**: the React/Vite/Tauri app shell is U1's deliverable and does not exist yet. This document is the *design contract* for U3's Queue UI; the actual components are implemented once the shell lands (or against a minimal harness at integration). During parallel dev the component calls the fixed Tauri commands, backed by U3 mocks.

## 1. Component Hierarchy
```
QueuePage
├─ QueueToolbar          # sort toggle (Priority / Newest), refresh, pending count
└─ QueueList
   └─ QueueItemCard[]     # one per pending item; inline-expandable
      ├─ ConfirmItemBody  # when kind = Confirm: candidate title/body preview
      │   └─ AnswerControls (Yes / No buttons + optional free text)
      └─ DeepenItemBody   # when kind = Deepen: question + hypothesis
          └─ AnswerControls (free text + optional suggested choices)
      └─ SkipAction        # leaves item pending
```
**Text alternative**: A page holds a toolbar (sort + refresh + count) and a list. Each list card renders a Confirm or Deepen item and exposes its answer controls inline; answering or skipping happens in place.

## 2. Props / State
- **QueuePage** (container)
  - state: `items: QueueItemDto[]`, `sort: QueueSort ("PriorityDesc"|"NewestFirst")`, `loading: boolean`, `answeringIds: Set<QueueItemId>`
  - effects: on mount + on `sort` change → `list_queue(sort)`.
- **QueueItemCard** (presentational)
  - props: `item: QueueItemDto`, `onAnswer(id, AnswerInput)`, `onSkip(id)`, `busy: boolean`
  - local state: `draftText: string`, `expanded: boolean`
- **AnswerControls**
  - Confirm: `Choice("yes")` / `Choice("no")` buttons; optional `Text(draft)` submit.
  - Deepen: `Text(draft)` submit; optional suggested `Choice` chips.

## 3. Interaction Flow (no context switch)
1. List renders pending items sorted by the toolbar setting.
2. User answers a card inline:
   - Confirm → tap **Yes** (`AnswerInput::Choice("yes")`) or **No** (`Choice("no")`), or type a correction and submit (`Text`).
   - Deepen → type an answer and submit (`Text`).
3. Card enters `busy`; call `answer_item(id, answer)`.
4. On `AnswerResult`:
   - Remove the answered card (optimistic).
   - If `follow_ups` non-empty, they appear in the list (re-fetch or merge) — the derived deepen questions surface immediately.
5. **Skip** removes the card from view but the item stays pending server-side (US-4.2/AC2); a later refresh shows it again.
6. Never navigate away — everything happens in the same view (US-4.2/AC1).

## 4. Backend integration (Tauri commands)
| Command | Purpose | Owner |
|---|---|---|
| `list_queue(sort: QueueSort) -> QueueItemDto[]` | Load pending items | U1 CommandRouter → U3 `InterviewService.list` |
| `answer_item(item_id, answer: AnswerInput) -> AnswerResult` | Submit answer / skip | U1 CommandRouter → U3 `InterviewService.answer` |

Types come from `src/shared/contracts.ts` (Milestone 0). Note serde tagging: `AnswerInput` is `{ "Choice": "yes" }` / `{ "Text": "..." }` / `"Skip"`; `QueueItemKind` is `{ "Confirm": { candidate } }` / `{ "Deepen": { question, hypothesis } }`.

## 5. Automation-friendly test IDs (stable `data-testid`)
- `queue-list`
- `queue-item-card-{id}`
- `queue-item-confirm-yes` / `queue-item-confirm-no`
- `queue-item-answer-input`
- `queue-item-answer-submit`
- `queue-item-skip`
- `queue-sort-toggle`
- `queue-pending-count`

## 6. Accessibility / UX notes
- Choice buttons and the text input are reachable by keyboard; submit on Enter.
- Skipping is non-destructive and reversible (item remains in the queue).
- Empty state: "대기 중인 질문이 없습니다" when the queue is empty.
