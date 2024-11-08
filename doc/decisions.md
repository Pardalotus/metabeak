# Decision Register for Pardalotus Metabeak

## 1: V8 for functions

### Objective: Allow user-supplied queries.

V8's Rust bindings were released late 2024, which was part of the inspiration for this project.

### Options considered

1. Simple filters
2. Custom query language
3. Existing query language
4. User-supplied functions in embedded Python
5. User-supplied functions in embedded JavaScript
7. User-supplied functions in embedded Lua

### Decision

5, embedded JavaScript with V8.

## 2: Different V8 isolate per user

Trust that the isolation is safe. But belt and braces.
