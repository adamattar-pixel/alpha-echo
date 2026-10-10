# Alpha Echo — UI Aesthetic Direction

## Goal

Refine the existing portfolio decision desk without changing its information architecture or behaviour. The interface should feel calm, premium, and editorial: a personal investment operating system rather than a dense trading terminal.

## Visual principles

- **White-first canvas:** warm white background with large quiet areas; no dark application chrome.
- **Soft enclosure:** cards use generous radii, thin cool-gray borders, and restrained shadows rather than heavy containers.
- **Editorial hierarchy:** black is reserved for decisions, values, and primary labels; gray carries metadata, explanations, timestamps, and secondary controls.
- **One atmospheric accent:** a low-saturation mist blue / sea-glass gradient is used only in the portfolio summary and key analytical visual, never as decoration everywhere.
- **Precision over density:** show the current portfolio conclusion first, then the holdings that explain it; detailed operations remain available but visually recessive.

## Tokens

| Role | Value |
| --- | --- |
| Canvas | `#F8F8F6` |
| Surface | `#FFFFFF` |
| Ink | `#101114` |
| Secondary text | `#7D8087` |
| Hairline | `#E8E9EA` |
| Cool accent | `#9DB9C4` |
| Accent tint | `#E7F0F1` |
| Positive | `#1A7F63` |
| Caution | `#B8751A` |
| Radius / large | `28px` |
| Radius / medium | `18px` |
| Radius / control | `12px` |

## Main decision desk

1. A compact floating top navigation identifies Alpha Echo, shows the active section, and keeps the global controls quiet.
2. A hero summary card presents total portfolio value, daily move, report freshness, and a soft abstract market-wave visual.
3. The primary decision signal is a single editorial panel: concise market posture, confidence, and next review time. It must never imply trade execution.
4. Holdings form calm rounded cards with price, cost, P&L, thesis status, and one-line evidence context. Positive and negative states use restrained color rather than full-color tiles.
5. Market intelligence uses two asymmetric cards: macro conditions and cross-market exposure. The section stays legible with no chart clutter.
6. The quick action rail keeps “record trade” and “generate report” visible without competing with portfolio decisions.

## Interaction states

- Hover: a 1–2px visual lift, never aggressive glow.
- Loading: gray skeletons that preserve layout.
- Empty: plain-language prompt with one clear next action.
- Partial report: explicit data-gap label, never a simulated market conclusion.
- Error: inline black/gray message and retry control; avoid alarming full-screen states.

## Typography

Use an expressive, high-contrast sans-serif display face for numeric portfolio values and page titles; pair it with a neutral grotesk for interface labels. Headlines are tight and large; metadata is small, letter-spaced, and gray.
