---
sidebar_position: 10
---

# Tray

This module provides a system tray for displaying icons of running applications.

Clicking on an icon will open the corresponding application or menu. The module only appears when applications have tray icons.

If another status bar or desktop component already provides the tray service
(`org.kde.StatusNotifierWatcher`), ashell uses it instead of replacing it, and
takes over when it exits. Several bars, or several ashell instances, then show
the same icons.

## Blocklist

You can filter which tray icons are displayed using the `blocklist` option. If a tray item's name matches any regex pattern in the blocklist, it won't be rendered.

**Note**: Matching is done against the tray item's name using regex patterns.

## Click Behavior

You can configure what happens when right-clicking a tray icon using `right_click`. The left click behavior is automatically set to the complement. If omitted, only left click is active and opens the context menu.

- `"Open"` — right click activates the application (e.g. show/raise its window); left click opens the context menu
- `"Menu"` — right click opens the context menu; left click activates the application

Applications without a menu are activated by any click.

## Icons

Each icon is chosen from what the application publishes, in this order:

1. the icon file, when the application gives an absolute path as its icon name
2. the icon name, looked up in the application's own icon directory (`IconThemePath`)
3. the icon name, looked up in your icon theme
4. the image data sent by the application
5. a similar icon name from your icon theme or installed applications

If nothing matches, a small dot is shown. Named icons come first so that the
icon follows your theme, as with other status bars.

## Examples

**Hide multiple applications by pattern:**

```toml
[tray]
blocklist = ["spotify", "^org\\.gnome\\."]
```

**Right click to open the context menu (left click opens app):**

```toml
[tray]
right_click = "Menu"
```

## Default Configuration

The default configuration is:

```toml
[tray]
blocklist = []
```
