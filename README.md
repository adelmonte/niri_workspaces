# niri_workspaces [![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

A Waybar module for displaying and managing Niri workspaces with pie chart icons.

![screenshot](demo.png)

## Features

- Pie chart icons showing window count per workspace (Nerd Font hexagonal icons)
- Custom format strings with placeholders (`{icon}`, `{value}`, `{name}`, `{index}`, `{output}`)
- Custom icons by workspace state and name
- Clickable workspace navigation (can be disabled)
- Drag and drop workspace reordering
- Multi-monitor support (current output or all outputs)
- Window filtering via ignore rules
- CSS styling with state-based classes

## Installation

### From AUR (Arch Linux)

```bash
yay -S niri_workspaces
```

The compiled module will be at `/usr/lib/waybar/libniri_workspaces.so`.

### Manual Installation

```bash
cargo build --release
```

The compiled module will be at `target/release/libniri_workspaces.so`.

## Configuration

### Basic Example

```jsonc
{
  "modules-left": ["cffi/niri_workspaces"],
  "cffi/niri_workspaces": {
    "module_path": "/home/user/.config/waybar/modules/libniri_workspaces.so",
    "show_empty_workspace": true,
    "icon_size": "large",
    "ignore_rules": [
      {"app_id": "xpad"},
      {"app_id": "firefox", "title": "Picture-in-Picture"}
    ]
  }
}
```

### Configuration Options

#### Display & Behavior

| Option | Description | Default |
|--------|-------------|---------|
| `all_outputs` | Show workspaces from all outputs on every bar | `false` |
| `show_empty_workspace` | Show the next empty workspace after occupied ones | `true` |
| `current_only` | Show only the active/focused workspace | `false` |
| `disable_click` | Disable click-to-switch navigation | `false` |

#### Formatting

- **`format`** (string, optional)
  - Custom format string with placeholders:
    - `{icon}` - Icon from format-icons or pie chart
    - `{value}` - Workspace name if named, otherwise index
    - `{name}` - Workspace name (empty if unnamed)
    - `{index}` - Workspace index on its output
    - `{output}` - Output name where workspace is located
  - Example: `"{icon} {name}"`, `"{output}:{value}"`
  - Default: Just the icon

- **`format-icons`** (object, optional)
  - Define custom icons based on workspace state and name
  - Icon priority (highest to lowest):
    1. `urgent` - For workspaces with urgent windows
    2. `empty` - For workspaces with no windows
    3. `focused` - For the currently focused workspace
    4. `active` - For workspaces active on their output
    5. Named workspace icons (e.g., `"browser": ""`)
    6. Index-based icons (e.g., `"1": ""`)
    7. `default` - Fallback icon
  - If no format-icons configured, defaults to window count pie charts
  - Example:
    ```jsonc
    "format-icons": {
      "urgent": "",
      "focused": "●",
      "active": "○",
      "empty": "",
      "browser": "",
      "chat": "",
      "default": ""
    }
    ```

- **`icon_size`** (string, optional)
  - Values: `"small"`, `"large"`, `"x-large"`, or sizes like `"14pt"`
  - Default: Theme's default font size

#### Window Filtering

- **`ignore_rules`** (array, default: `[]`)
  - Hide specific windows from workspace counts
  - Each rule can have:
    - `app_id` (string) - Exact app ID match
    - `title` (string) - Exact window title match
  - All matchers in a rule must match (AND logic). Multiple rules use OR logic.
  - Example:
    ```jsonc
    "ignore_rules": [
      {"app_id": "xpad"},
      {"app_id": "firefox", "title": "Picture-in-Picture"},
      {"title": "Firefox — Sharing Indicator"}
    ]
    ```

### More Examples

**With custom format and icons:**
```jsonc
{
  "modules-left": ["cffi/niri_workspaces"],
  "cffi/niri_workspaces": {
    "module_path": "/home/user/.config/waybar/modules/libniri_workspaces.so",
    "format": "{icon} {name}",
    "format-icons": {
      "urgent": "",
      "focused": "●",
      "active": "○",
      "browser": "",
      "discord": "",
      "chat": "",
      "default": ""
    }
  }
}
```

**Multi-monitor (all workspaces on all bars):**
```jsonc
{
  "modules-left": ["cffi/niri_workspaces"],
  "cffi/niri_workspaces": {
    "module_path": "/home/user/.config/waybar/modules/libniri_workspaces.so",
    "all_outputs": true,
    "format": "{output}:{value}"
  }
}
```

**Minimal (current workspace only):**
```jsonc
{
  "modules-left": ["cffi/niri_workspaces"],
  "cffi/niri_workspaces": {
    "module_path": "/home/user/.config/waybar/modules/libniri_workspaces.so",
    "current_only": true,
    "format": " {name}"
  }
}
```

## Pie Icons

The module uses Nerd Font hexagonal pie icons to represent window counts:

| Count | Icon | Description |
|-------|------|-------------|
| 0 | 󰋙 | Empty workspace |
| 1 | 󰫃 | 1/8 filled |
| 2 | 󰫄 | 2/8 filled |
| 3 | 󰫅 | 3/8 filled |
| 4 | 󰫆 | 4/8 filled |
| 5 | 󰫇 | 5/8 filled |
| 6 | 󰫈 | 6/8 filled |
| 7+ | 󰫈 | Color-coded by count |

**Color coding for 7+ windows:**
- 7 windows: Red (`#bf616a`)
- 8 windows: Orange (`#d08770`)
- 9 windows: Yellow (`#ebcb8b`)
- 10 windows: Green (`#a3be8c`)
- 11 windows: Blue (`#81a1c1`)
- 12 windows: Purple (`#b48ead`)
- 13 windows: Brown (`#8b7355`)
- 14 windows: Grey (`#808080`)
- 15+ windows: Black (`#000000`)

**Note:** Requires a Nerd Font for proper icon rendering.

## Styling

Customize appearance using Waybar's GTK CSS. The module container uses class `.niri_workspaces` and contains `button.workspace-button` elements.

**IMPORTANT:** CFFI modules require the `cffi.` prefix in CSS selectors with an escaped dot. If you named it `cffi/niri_workspaces` in your Waybar config, use `#cffi\.niri_workspaces` in your CSS (note the backslash).

### Available CSS Classes

**State Classes:**
- `.workspace-button` - All workspace buttons
- `.focused` - The single focused workspace (across all outputs)
- `.active` - Workspace is active/visible on its output (but might not be focused)
- `.urgent` - Workspace has an urgent window
- `.empty` - Workspace has no windows
- `.current_output` - Workspace is on the same output as the bar
- `.dragging` - Workspace being dragged
- `.drag-over` - Valid drop target during drag

**Widget Names:**
Each button also has a widget name for CSS targeting:
- `#niri-workspace-<name>` - For named workspaces (e.g., `#niri-workspace-browser`)
- `#niri-workspace-<index>` - For unnamed workspaces (e.g., `#niri-workspace-1`)

### Example Styles

**Minimal with state indicators:**
```css
#cffi\.niri_workspaces {
  background-color: transparent;
  margin: 0;
  padding: 0;
}

#cffi\.niri_workspaces button {
  padding: 0 8px;
  margin: 0 1px;
  background-color: transparent;
  border-top-left-radius: 5px;
  border-top-right-radius: 5px;
  color: #ffffff;
  transition: background-color 0.2s ease, box-shadow 0.2s ease;
  box-shadow: inset 0 -3px 0 0 transparent;
}

#cffi\.niri_workspaces button:hover {
  background-color: rgba(255, 255, 255, 0.1);
}

/* Order matters! More specific states should come last */
#cffi\.niri_workspaces button.active {
  background-color: rgba(95, 103, 118, 0.5);
}

#cffi\.niri_workspaces button.focused {
  background-color: rgba(95, 103, 118, 1);
  box-shadow: inset 0 -3px 0 0 #81a1c1;
}

#cffi\.niri_workspaces button.urgent {
  background-color: rgba(191, 97, 106, 0.5);
  box-shadow: inset 0 -3px 0 0 #bf616a;
  animation: urgent-blink 1s ease-in-out infinite;
}

#cffi\.niri_workspaces button.empty {
  opacity: 0.6;
}

@keyframes urgent-blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
```

**With drag and drop indicators:**
```css
#cffi\.niri_workspaces button.dragging {
  opacity: 0.6;
  background-color: rgba(102, 204, 255, 0.3);
}

#cffi\.niri_workspaces button.drag-over {
  background-color: rgba(102, 255, 153, 0.2);
  border: 1px dashed rgba(102, 255, 153, 0.6);
}
```

**Target specific named workspaces:**
```css
/* Style the browser workspace differently */
#cffi\.niri_workspaces button#niri-workspace-browser {
  color: #ff7f50;
}

/* Style the first workspace */
#cffi\.niri_workspaces button#niri-workspace-1 {
  font-weight: bold;
}
```
## License

GPL-3.0-or-later
