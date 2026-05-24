# AML Interface Resolution

## Overview

Resolution is the process of binding an `Invocation` node to a concrete
implementation. It runs during Phase 4 of the lifecycle (after validation
and definition registration).

## Resolution Algorithm

```
resolve(node, registry) → Implementation | Error
```

### Step 1: Determine resolution mode

| Attributes present | Mode |
|---|---|
| `impl` (with or without `interface`) | **Direct** |
| `interface` (without `impl`) | **Interface** |
| `name` only | **Lookup** |

### Step 2: Execute resolution

#### Direct Mode (`impl` present)

```
1. implementation = registry.get_implementation(node.impl)
2. if implementation is None:
     return Error::NotFound(node.impl)
3. if node.interface is Some(iface):
     if implementation.implements != iface:
       return Error::ImplInterfaceMismatch(node.impl, iface, implementation.implements)
4. return implementation
```

#### Interface Mode (`interface` present, no `impl`)

```
1. candidates = registry.get_implementations_for(node.interface)
2. if candidates is empty:
     return Error::NoImplementation(node.interface)
3. if node.language is Some(lang):
     candidates = candidates.filter(|c| c.language == lang)
4. if node.framework is Some(fw):
     candidates = candidates.filter(|c| c.framework == fw)
5. if candidates.len() == 1:
     return candidates[0]
6. if candidates.len() == 0:
     return Error::NoImplementation(node.interface)  # after filtering
7. if candidates.len() > 1:
     # Apply priority: check registry-configured default
     if registry.has_default_for(node.interface):
       default = registry.get_default(node.interface)
       if default in candidates:
         return default
     return Error::Ambiguous(node.interface, candidates)
```

#### Lookup Mode (`name` only)

```
1. # Try implementation first
   implementation = registry.get_implementation(node.name)
   if implementation is Some:
     return implementation
2. # Then try interface (resolve as if interface= was used)
   interface = registry.get_interface(node.name)
   if interface is Some:
     return resolve_interface(node.name, registry, hints=None)
3. return Error::NotFound(node.name)
```

### Step 3: Validate binding

After resolution succeeds, the executor validates:
- The implementation is compatible with the invocation's expected input/output.
- Any capability requirements are met (see `security.md`).

## Hint Matching

Hints narrow the candidate set during Interface Mode resolution.

| Hint | Matching rule | Example |
|---|---|---|
| `language` | Exact string match against implementation's `language` attribute | `language="python"` |
| `framework` | Exact string match against implementation's `framework` attribute | `framework="pytest"` |

Hints are **conjunctive** (AND logic): all provided hints must match for a
candidate to pass filtering.

Hints are **optional**: if no hints are provided, all implementations for the
interface are candidates.

## Registry Defaults

The `SkillRegistry` supports configuring a default implementation per interface:

```rust
registry.set_default("unit-testing-coverage", "python-pytest-v2");
```

Defaults are used as a tie-breaker ONLY when multiple candidates remain after
hint filtering. They do NOT override explicit `impl=` bindings.

## Resolution Errors

| Error | Cause | Recovery |
|---|---|---|
| `NotFound` | No implementation or interface with the given name | Check spelling, ensure package installed |
| `NoImplementation` | Interface exists but no implementation matches hints | Install an implementation or broaden hints |
| `Ambiguous` | Multiple implementations match; no default configured | Add hints or configure a default |
| `ImplInterfaceMismatch` | `impl` does not implement the declared `interface` | Fix the impl or interface attribute |

All resolution errors are **fatal for the node** — the node cannot execute.
The executor's failure propagation rules (see `execution.md`) determine whether
the error is fatal for the entire document.

## Determinism Guarantee

Given:
- The same `SkillRegistry` state (same registered interfaces, implementations, defaults)
- The same document (same attributes on the same nodes)

Resolution ALWAYS produces the same binding or the same error. There is no
randomness, no ordering dependency between nodes, and no external state
consulted during resolution.

## Namespace Rules

Implementation and interface names occupy **separate namespaces**:
- An interface named `foo` and an implementation named `foo` can coexist.
- In Lookup Mode, implementation is checked first, then interface.

Names within each namespace are **package-scoped** when multiple packages are
installed. The fully-qualified form is `<package>/<name>` (e.g. `aml-stdlib/unit-testing-coverage`).
Short names (without package prefix) resolve within the current package first,
then search all packages.

Collision rules:
- Two interfaces with the same name from different packages → last registered wins (with warning).
- Two implementations with the same name from different packages → last registered wins (with warning).
- Collision within the same package → `RegistrationError::Duplicate` (hard error).
