## 1. HTML: Remove inline handlers and add button IDs

- [x] 1.1 In `settings.html`, replace `<button onclick="saveGeneral()">Save</button>` with `<button id="btn-save-general">Save</button>` in the General panel
- [x] 1.2 In `settings.html`, replace `<button onclick="saveAdvanced()">Save</button>` with `<button id="btn-save-advanced">Save</button>` in the Advanced panel
- [x] 1.3 In `settings.html`, replace `<button onclick="addMount()">Add Mount</button>` with `<button id="btn-add-mount">Add Mount</button>` in the Mounts panel
- [x] 1.4 In `settings.html`, replace `<button class="btn-danger" onclick="signOut()">Sign Out</button>` with `<button id="btn-sign-out" class="btn-danger">Sign Out</button>` in the Account panel
- [x] 1.5 In `settings.html`, replace `<button class="btn-danger" onclick="clearCache()">Clear Cache</button>` with `<button id="btn-clear-cache" class="btn-danger">Clear Cache</button>` in the Advanced panel

## 2. JS: Wire event listeners

- [x] 2.1 In `settings.js`, add `addEventListener` calls after the existing `loadSettings(); loadMounts();` block to wire all 5 buttons: `btn-save-general` → `saveGeneral`, `btn-save-advanced` → `saveAdvanced`, `btn-add-mount` → `addMount`, `btn-sign-out` → `signOut`, `btn-clear-cache` → `clearCache`

## 3. Verify

- [x] 3.1 Confirm no `onclick=`, `onsubmit=`, or other inline `on<event>=` attributes remain in any `.html` file under `crates/cloudmount-app/dist/`
- [x] 3.2 Run the UX reviewer agent against `crates/cloudmount-app/dist/` and confirm zero BLOCKED issues
fiolesystemsyn(0X800700A1àUNABLE_TO_MASK_PATHc