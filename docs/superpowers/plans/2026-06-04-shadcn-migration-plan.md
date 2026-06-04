# Plan: Migrate DRBD-HA UI from Ant Design to shadcn/ui (Radix)

## Goal
Remove all Ant Design usage from `drbd-ha/ui` and replace with shadcn/ui components
(Radix primitives + Tailwind v4). Keep the orange brand (`#F79133`) as `primary`.
Preserve all existing custom CSS animations, GSAP entrance animations, and glass effects.

## Constraints
- Build tool stays **rsbuild**; Tailwind v4 stays; `@ → src` alias stays; `/api` proxy stays.
- shadcn components are copied into `src/components/ui/*` (manual install, not an npm dep).
- No `react-hook-form`: keep existing controlled-state forms, just swap inputs.
- `antd` `message.*` → `sonner` `toast`. `@ant-design/icons` → `lucide-react`.
- `Steps` → custom `ui/stepper`; `Empty`/`Result` → small custom components (shadcn has none).
- Theme switching keeps the existing zustand `theme` store, driven via `.dark` class on `<html>`.

## Acceptance (whole project)
- `grep -rE "antd|@ant-design" drbd-ha/ui/src` returns nothing.
- `package.json` has no `antd` / `@ant-design/icons`; `npm install` clean.
- `npm run build` succeeds in `drbd-ha/ui`.
- `cargo build -p drbd-ha` succeeds (UI dist re-embeds).
- Server starts and `http://127.0.0.1:3373/` returns HTTP 200.

---

## Task 1 — Infrastructure + base ui/* components (foundation, capable model)
Set up everything every other task depends on.
1. Add deps: `class-variance-authority`, `clsx`, `tailwind-merge`, `lucide-react`, `sonner`,
   and Radix primitives: `@radix-ui/react-slot`, `react-dialog`, `react-select`, `react-tabs`,
   `react-progress`, `react-tooltip`, `react-label`. Remove `antd`, `@ant-design/icons` from package.json.
2. Create `src/lib/utils.ts` exporting `cn()` (clsx + tailwind-merge). Create `components.json`.
3. In `src/index.css`, add the shadcn CSS-variable layer (`:root` light + `.dark`) mapping
   `--primary` to the orange brand `#F79133` (oklch/hex acceptable) plus
   `--background/foreground/card/popover/muted/accent/destructive/border/input/ring/radius`.
   **Do not remove** any existing animations, glass, or utility classes already in the file.
   Remove only the `/* Custom Ant Design Overrides */` block (`.ant-*` rules).
4. Build `src/components/ui/`: `button.tsx`, `card.tsx`, `dialog.tsx`, `input.tsx`, `label.tsx`,
   `select.tsx`, `tabs.tsx`, `badge.tsx`, `progress.tsx`, `tooltip.tsx`, `spinner.tsx` (Loader2),
   `sonner.tsx` (Toaster), plus custom `stepper.tsx`, `empty.tsx`, `result.tsx`.
   Use canonical shadcn implementations adapted to Tailwind v4 CSS vars.
5. Verify: `npm run build` passes (no consumers yet, but components must compile).

## Task 2 — App shell: index.tsx, App.tsx, router.tsx, MainLayout.tsx (standard model)
- Replace antd `Layout/Header/Content`, `ConfigProvider`, `Button`, `Tooltip` in `MainLayout.tsx`
  with div + Tailwind + `ui/button` + `ui/tooltip`. Keep GSAP header animation and theme toggle.
- Remove antd `ConfigProvider` wrapper wherever it lives (App/MainLayout); mount `<Toaster richColors />`.
- Swap `@ant-design/icons` (`ApiOutlined`, `MoonOutlined`, `SunOutlined`, etc.) to lucide equivalents.
- Verify `npm run build`.

## Task 3 — Pages batch A: Nodes, Resources, Storage, HaProfiles, Logs (standard model)
- Replace antd components per the mapping table. `message.*` → `toast`.
- Tables: these pages render data; keep current markup, swap antd Card/Tag/Button/Spin/Empty/Select.
- Verify `npm run build`.

## Task 4 — Wizard: Wizard.tsx, ServiceHaWizard.tsx, OcfAgentEditorPage.tsx + step components (capable model)
- step components: ActivationStep, DeploymentStatusStep, HaConfigStep, NodesVerificationStep,
  OcfAgentModal, PreviewConfigStep, StorageConfigStep.
- antd `Steps` → `ui/stepper`; `Modal` → `ui/dialog`; `Form/Input/Select` → ui equivalents; `Result` → `ui/result`.
- Verify `npm run build`.

## Task 5 — OCF agent editor + hooks: OcfAgentEditor, AddAgentModal, AddParameterModal, AgentPreview, SortableAgentItem; hooks useWizardPersist/useStoragePools (standard model)
- Replace antd in these components; `message.*` in hooks → `toast`. Keep dnd-kit usage intact.
- Verify `npm run build`.

## Task 6 — Cleanup + full verification (standard model)
- `npm install` to drop antd from lockfile. Confirm acceptance grep is empty.
- `npm run build`, then `cargo build -p drbd-ha`, start server, curl `/` → 200.
