---
name: sova-ui-perfection
description: "SOVA UI specialist enforcing Apple/Trade Republic/Linear design. Read the Design Bible at artifacts/sova_design_bible.md BEFORE touching any frontend code. Triggers: UI, design, component, feed, wallet, dashboard, social, SOVA, styling, CSS."
metadata:
  version: "3.0.0"
  domain: frontend
  triggers: UI, Svelte, SOVA, design, component, feed, wallet, CSS, styling
  role: specialist
  scope: implementation
  output-format: code
---

# SOVA UI Perfection — Apple/Trade Republic/Linear Level

## FIRST: Read the Design Bible
**BEFORE writing ANY frontend code**, read `.agent/design/sova_design_bible.md`.
It contains mockup images (4 PNGs in `.agent/design/`), exact pixel specifications, and justifications from Apple HIG 2026, Material Design 3, Dieter Rams, Trade Republic, Linear, and Stripe.

## Quick Reference — Design Tokens

```css
/* Surfaces */
--sova-bg-0: #000000;   --sova-bg-1: #0f0f0f;   --sova-bg-2: #1a1a1a;   --sova-bg-3: #2a2a2a;

/* Text */
--sova-text-0: #ffffff;  --sova-text-1: #a0a0a0;  --sova-text-2: #666666;

/* ONE accent */
--sova-accent: #00DC82;

/* Semantic */
--sova-positive: #00DC82;  --sova-negative: #FF4444;  --sova-warning: #FFB800;

/* Borders — ultra-subtle */
--sova-border: rgba(255,255,255,0.06);
```

## Typography — Inter ONLY
| Role | Size | Weight | Spacing |
|------|------|--------|---------|
| Hero number | 48px | 700 | -0.03em |
| Page title | 28px | 700 | -0.03em |
| Card value | 24px | 700 | -0.02em |
| Post title | 16px | 600 | -0.01em |
| Body | 14px | 400 | 0 |
| Section label | 11px | 600 | 0.06em, UPPERCASE |

## Spacing — 4px grid
4, 8, 12, 16, 20, 24, 32, 40, 48. Nothing else.

## NEVER DO (14 rules)
1. ❌ Gradients for decoration
2. ❌ Glassmorphism (blur, saturate)
3. ❌ Colored box-shadow (glow)
4. ❌ Second font family (Outfit)
5. ❌ Multiple accent colors
6. ❌ Sidebar navigation
7. ❌ SVG pattern identicons
8. ❌ Animations > 0.3s
9. ❌ Background tinted (#0a0a0f) — pure black only
10. ❌ Text < 11px
11. ❌ Buttons without 44×44 touch target
12. ❌ `var(--font-display)` — deleted
13. ❌ Classes: .glass, .aurora-bg, .card-aurora
14. ❌ Emoji in navigation

## Component Specs (see Bible for full details)
- **TopBar**: 48px, black bg, "SOVA" left, balance+dot right
- **NavBar**: 56px bottom, #0f0f0f, 5 items, Create=green circle
- **Feed posts**: NO card borders, separator lines only, 36px circle avatars
- **Wallet**: 48px centered balance, 3 action buttons, transaction list
- **Profile**: 48px avatar circle, 3-col stats grid, Settings-style info rows

## ALWAYS DO
- [ ] Read Design Bible before coding
- [ ] Use Svelte 5 Runes ($state, $derived, $effect, $props)
- [ ] Colors from CSS variables only
- [ ] Run `npm run ai:check` → 0/0
- [ ] Match mockup images pixel-by-pixel
