# DarwinPlay 0.6 Design System

## Direction

DarwinPlay uses a console-oriented desktop launcher layout with centered top navigation, medium information density and restrained motion. The interface prioritizes library artwork and play state while keeping runtime internals in Settings and Console.

## Palette

| Token | Value | Purpose |
| --- | --- | --- |
| Background | `#10171B` | Main canvas |
| Surface | `#172126` | Cards and navigation |
| Surface Raised | `#1D292F` | Selected and elevated surfaces |
| Border | `#2B3940` | Quiet separation |
| Text Primary | `#F1F4F2` | Primary labels |
| Text Secondary | `#A7B1AD` | Supporting labels |
| Text Tertiary | `#6F7C78` | Metadata |
| Accent | `#8FBA63` | Focus, play and active navigation |
| Success | `#70AE82` | Ready state |
| Warning | `#CDA65C` | Setup and degraded state |
| Error | `#CA7373` | Failure state |
| Info | `#72A5BA` | Informational state |

## Navigation

Primary navigation is centered at the top and contains Home, Games and Console. Settings is a utility action, not a primary destination. Games has Steam and Imported secondary tabs.

## Setup

Initial Wine and Steam installation appears on Home as two game-sized setup cards. Completed install actions disappear from normal navigation and runtime management moves to Settings.

## Library

Steam games use vertical 2:3 covers. Home uses a wide hero for the latest-played title. Cards show title, compatibility, last played, installed state and favorite state without exposing PE or backend internals by default.

## Motion

Hover scale stays below 1.02. Focus changes use color, opacity and subtle scale. No persistent glow or decorative pulse is used.

## Icon

The application mark combines a stylized Darwin finch with overlapping compatibility layers. It avoids controller imagery, letters and portraits so the mark remains recognizable at Dock sizes.
