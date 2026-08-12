# Daily Task Monitor UI Design Compass

This document is the visual and interaction baseline for the UI refresh. It is
not a mandate to clone another product. It records what Daily Task Monitor can
learn from Rize and Mole while preserving its local-first product identity.

## 1. Product character

Daily Task Monitor should feel like a **quiet instrument for understanding a
day**, not an administration console and not surveillance software.

The desired character is:

- calm, precise, and trustworthy;
- information-rich without feeling crowded;
- warm enough for daily use, but never decorative at the expense of evidence;
- locally owned: sensitive evidence is clearly presented as data on this device;
- progressively disclosed: summary first, evidence and controls on demand.

Three words should guide visual decisions: **calm, legible, humane**.

## 2. Reference products

### Rize: the analytical skeleton

Rize is the closer product reference because it turns automatically captured
activity into a daily timeline, project breakdowns, and productivity signals.
Its strongest transferable ideas are:

- Make the timeline the narrative spine of the day, not one chart among many.
- Put the most decision-relevant numbers first; defer detailed evidence.
- Keep application identity visible with icons and names so raw activity remains
  recognizable.
- Use consistent semantic color for focus, meetings, breaks, and uncategorized
  time instead of giving every widget its own palette.
- Present privacy as part of the experience, not only as policy text. Editing,
  excluding, and reviewing data should feel ordinary and safe.
- Prefer reviewable automatic tracking over timers and manual bookkeeping.

What not to copy:

- A productivity score must not become the product's moral judgement of the
  user. Daily Task Monitor should explain evidence and uncertainty.
- Team utilization, billing, and management language do not belong in the
  personal local-first experience unless a future opt-in workspace requires it.
- Dense dashboards should not force every metric above the fold.

### Mole: the emotional and interaction layer

Mole's website and product presentation pair restrained native-Mac framing with
a memorable planetary metaphor. Five planets give five tools one identity; a
short poetic line gives each task an emotional tone; the product explains what
will be inspected or changed before an action begins.

Its strongest transferable ideas are:

- Give each primary area one clear job and one recognizable visual anchor.
- Use a single focal illustration or data object per page, surrounded by ample
  quiet space. Avoid a wall of equal-weight cards.
- Combine plain operational copy with one short human line. Personality should
  appear at transitions and empty states, not inside every control.
- State scope and consequences before consequential actions. For this product,
  that means explaining what is collected, excluded, reclassified, exported, or
  sent to an optional AI provider.
- Use motion as feedback: scanning, refreshing, expanding evidence, and changing
  time range may animate; stable reading surfaces should stay still.
- Preserve platform conventions. Custom styling should not make buttons,
  selection, scrolling, dialogs, and keyboard focus unfamiliar.

Observed in the macOS application:

- The window uses a dark, warm olive-brown ambient field instead of neutral
  black. Thin translucent borders and low-contrast surfaces create depth without
  obvious shadows.
- Primary navigation is a centered floating pill. The selected item becomes a
  high-contrast light capsule, so location remains instantly readable.
- The status page uses a strict four-column grid. One large health card carries
  the page's expressive object (the Sun); the remaining cards use restrained
  sparklines, bars, and small semantic icons.
- Numbers are dominant, units are quieter, and supporting evidence sits on a
  final baseline. This makes every card scan in the same order.
- Live status colors are economical: green for healthy state, amber for load,
  red for heat or urgency, blue for capacity, and gray for inactive data.
- Dense process data changes visual grammar from cards to a compact table. This
  avoids forcing one component style onto every data density.
- A permission problem appears as a persistent bottom notice with a direct
  Settings action and dismiss control. It stays visible without blocking the
  rest of the dashboard.
- Platform chrome, tooltips, sortable columns, contextual row actions, and
  keyboard tab switching remain intact beneath the custom visual theme.

What not to copy:

- Planets are Mole's product metaphor. Daily Task Monitor needs its own language.
- Large decorative scenes are unsuitable for timeline and evidence-heavy pages.
- Poetic copy must not obscure errors, privacy implications, or destructive
  actions.
- Continuous ornamental animation would compete with concentration and charts.

## 3. Daily Task Monitor's own metaphor

The product metaphor is a **day atlas**:

- Today is the current route.
- The activity timeline is the track already travelled.
- Applications and categories are landmarks.
- Trends compare routes across days and weeks.
- Projects connect observed time to intended destinations.
- AI analysis is a field note based on selected evidence, never an oracle.

This metaphor should remain subtle. Use it in section naming, empty states,
transitions, and a small number of illustrations. Do not turn every control into
a map ornament.

## 4. Information architecture

Every primary screen should follow the same reading order:

1. **Orientation** — page name, date or range, collection state.
2. **Answer** — the one summary or visualization that answers the page's main
   question.
3. **Explanation** — compact breakdowns and comparisons.
4. **Evidence** — timeline segments, apps, URLs, titles, and confidence details.
5. **Action** — edit, filter, export, analyze, or configure.

Do not place several unrelated KPI strips before the page's main answer. A card
is justified only when it groups content with a distinct semantic boundary or
interaction; it is not the default container for every label and value.

## 5. Visual language

### Surfaces

- Use one app canvas, one raised panel level, and one transient overlay level.
- Prefer subtle tonal separation over heavy borders and drop shadows.
- Keep corner radii consistent. Recommended scale: 8 px for controls, 12 px for
  panels, and 16 px only for major hero or empty-state surfaces.
- Dividers are for aligning dense evidence, not outlining every region.

### Color

- The neutral canvas should dominate. Accent color indicates selection and the
  current temporal context.
- Reserve saturated colors for semantic data and status.
- A category keeps the same color in timelines, legends, charts, filters, and
  detail views.
- Never rely on color alone; pair it with label, icon, pattern, or position.
- Dark mode is a first-class tonal system, not an inverted light theme.

### Type

- The user's selected system font remains authoritative.
- Use no more than four practical text roles: page title, section title, body,
  and metadata.
- Use tabular numerals for durations, percentages, clock times, and aligned
  metrics. Monospace is useful for evidence-like values, not for general prose.
- Prefer weight and spacing to repeated size jumps. Secondary text must remain
  comfortably readable rather than merely faint.

### Icons

- Use a coherent stroke family for product actions and navigation.
- Keep real application icons faithful to their source; do not recolor them to
  match the interface.
- Every icon sits in a fixed optical box. Center the visible artwork, accounting
  for transparent padding in source files.
- An icon without an accessible label is allowed only when its meaning is
  universal and a tooltip is present.

### Motion

- Fast feedback: 120–180 ms for hover, press, selection, and small disclosure.
- Structural changes: 180–260 ms for panels, filters, and range transitions.
- Use opacity and small transforms; avoid large sliding distances.
- Respect reduced-motion settings. No looped motion in analytical views.

## 6. Core component rules

### Navigation

- One stable primary sidebar; current location is visible without relying on
  icon color alone.
- Badges communicate actionable state, not decoration.
- Settings are secondary and should never compete with daily review.

### Timeline

- The timeline is visually dominant on Today.
- Segment geometry must remain stable while details open.
- Hover or selection reveals exact time, duration, application, category, and
  local evidence. Sensitive titles and URLs should be easy to hide.
- Gaps and unknown periods are honest states, not silently interpolated data.

### Metrics and charts

- Each chart states its question in the title or supporting sentence.
- Legends are close to the data and use the same ordering as the visualization.
- Default to direct labels where they improve scanning.
- Empty, loading, partial, and stale states must be designed explicitly.
- Comparisons state both the period and the basis; percentages without a valid
  baseline are omitted.

### Controls and settings

- Common choices are immediately visible; advanced controls use disclosure.
- Settings load instantly from startup-cached capabilities. Expensive system
  discovery must not run whenever the page opens.
- Inputs preserve native keyboard behavior and visible focus.
- Consequential actions preview scope, while reversible edits stay lightweight.

### AI surfaces

- Clearly separate observations, possible explanations, and verification steps.
- Show evidence range, generation state, and whether any data may leave the
  device before the user opts in.
- AI output must not visually outrank authoritative local metrics.

## 7. Writing style

Use concise, concrete Chinese. Lead with the answer and place technical detail
behind disclosure.

- Good: “今天有 2 小时 14 分钟处于专注状态。”
- Good: “这段时间没有足够证据，暂未分类。”
- Avoid: “您的生产力很差。”
- Avoid: vague labels such as “智能优化” without saying what changes.

Human warmth belongs in empty states and completion moments. Errors and privacy
prompts should be literal and unambiguous.

## 8. Review checklist

Before merging a UI change, verify:

- Can a user identify the page's main answer within five seconds?
- Is the dominant object obvious, or are several cards competing equally?
- Does every color have a stable semantic meaning?
- Are data, interpretation, and action visually distinct?
- Are loading, empty, stale, partial, and error states covered?
- Can the flow be completed with keyboard and visible focus?
- Does it remain legible with long Chinese labels and a custom system font?
- Are sensitive window titles, paths, and URLs handled deliberately?
- Does motion communicate change, and does reduced motion still work?
- Is this recognizably Daily Task Monitor rather than a copy of Rize or Mole?

## 9. Initial direction for the refresh

The first implementation pass should focus on foundations rather than isolated
page decoration:

1. Normalize color, spacing, radius, typography, elevation, and motion tokens.
2. Rebuild the application shell and primary navigation around a calmer canvas.
3. Make Today timeline-first and remove equal-weight card clutter.
4. Unify application identity, category colors, chart legends, and evidence
   details.
5. Apply the same hierarchy to Trends, Projects, AI review, and Settings.

This order keeps later pages from inventing their own visual dialects.

## Sources reviewed

- Mole product site: <https://mole.fit/zh/>
- Rize product site: <https://rize.io/>
