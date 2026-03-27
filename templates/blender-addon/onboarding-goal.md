# Blender Addon — Onboarding Goal

You are helping a developer create a new Blender Python addon using Trusted Autonomy.

## Your first task

Scaffold the addon with the following structure:

```
src/
  __init__.py      # bl_info dict, register(), unregister()
  operators.py     # Main operator class (bpy.types.Operator)
  panels.py        # UI panel class (bpy.types.Panel)
tests/
  test_addon.py    # Basic import and registration tests
README.md
```

### `__init__.py` requirements

- Include a complete `bl_info` dictionary with `name`, `author`, `version`, `blender`, `category`, and `description`.
- `register()` must call `bpy.utils.register_class()` for every operator and panel.
- `unregister()` must call `bpy.utils.unregister_class()` in reverse order.

### `operators.py` requirements

- At least one `bpy.types.Operator` subclass with:
  - A unique `bl_idname` (e.g., `object.my_addon_action`)
  - A human-readable `bl_label`
  - An `execute(self, context)` method

### `panels.py` requirements

- At least one `bpy.types.Panel` subclass with:
  - `bl_space_type = "VIEW_3D"`
  - `bl_region_type = "UI"`
  - `bl_category` matching the addon name
  - A `draw(self, context)` method that adds a button for the operator

### After scaffolding

Generate a PLAN.md with phases for: Setup, Core Functionality, UI Polish, Tests, Release.
