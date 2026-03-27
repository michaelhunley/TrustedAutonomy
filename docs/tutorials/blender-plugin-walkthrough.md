# Blender Plugin Walkthrough

This tutorial walks you through creating a Blender Python addon using Trusted Autonomy's `blender-addon` template.

## Prerequisites

- TA CLI installed (`ta --version`)
- Blender 3.x or 4.x installed (for testing the addon)
- Python 3.10+ available on `PATH`

## Step 1 — Install the template

The `blender-addon` template is built into TA. You can view all templates with:

```bash
ta template list
```

To install a community template:

```bash
ta template install blender-addon
```

## Step 2 — Create a new project

```bash
ta new run --template blender-addon --name my-blender-addon
cd my-blender-addon
```

This creates a directory with:
- `src/__init__.py` — addon entry point
- `.ta/workflow.toml` — TA project config
- `.taignore` — files to exclude from diffs
- `PLAN.md` — development roadmap

## Step 3 — Start the onboarding goal

The template includes an onboarding goal prompt. Run it to scaffold the full addon structure:

```bash
ta run "Scaffold addon structure"
```

The agent will:
1. Create `bl_info`, `register()`, and `unregister()` in `src/__init__.py`
2. Add a `bpy.types.Operator` in `src/operators.py`
3. Add a `bpy.types.Panel` in `src/panels.py`
4. Add basic tests in `tests/test_addon.py`

## Step 4 — Review the draft

When the agent finishes, review its changes:

```bash
ta draft list
ta draft view <id>
```

Or use the web UI:

```bash
ta daemon start
# Open http://localhost:7700/ui in your browser
```

## Step 5 — Approve and apply

```bash
ta draft approve <id>
ta draft apply <id>
```

Or use `ta publish` for a one-step commit + push:

```bash
ta publish --message "feat: scaffold blender addon"
```

## Step 6 — Test in Blender

1. Zip the `src/` directory: `zip -r my_addon.zip src/`
2. Open Blender > Edit > Preferences > Add-ons > Install
3. Select the zip file and enable the addon

## Next Steps

Continue with additional goals:

```bash
ta run "Add a custom mesh operator"
ta run "Add property group with settings panel"
ta run "Add unit tests for register/unregister"
```

Use `ta plan wizard` to create a phased roadmap for your addon development.
