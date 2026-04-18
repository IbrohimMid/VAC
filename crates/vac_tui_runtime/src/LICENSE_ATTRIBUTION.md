# License Attribution

## Stakpak TUI

Portions of this TUI implementation are derived from Stakpak (https://github.com/stakpak/agent).

Original work Copyright (c) Stakpak Contributors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

## Modifications

The following modifications have been made to the original Stakpak TUI code:

1. Re-branded from "Stakpak" to "VAC (Vastar Agentic CLI)"
2. Adapted types to use VAC's internal type system
3. Added VAC-specific commands (/vil, /swarm, /rulebook, /context, /runtime)
4. Integrated with VAC's VacEngine runtime
5. Modified session semantics to use VAC's restore-first approach

## Files Derived from Stakpak

- `event.rs` - Event mapping (adapted)
- `terminal.rs` - Terminal guard (adapted)
- `constants.rs` - UI constants (adapted)
- `services/` - UI services (transplanted, not yet activated)
- Layout and interaction patterns (adapted)