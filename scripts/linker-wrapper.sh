#!/bin/bash
# This is the truly definitive linker wrapper. It meticulously filters out any
# problematic `-lstdc++` flags from rustc, and then appends a self-contained,
# correctly-ordered static linking block to ensure all runtime libraries are linked statically.

# Accumulate arguments into an array, filtering as we go.
final_args=()
for arg in "$@"; do
  # Filter out the standalone `-lstdc++` which rustc or its dependencies might add.
  if [ "$arg" != "-lstdc++" ]; then
    final_args+=("$arg")
  fi
done

# Now, execute the real g++ linker with the filtered arguments,
# and append our definitive static linking block at the very end.
# This version adds -static-libgcc and links pthread to resolve
# the '__emutls_get_address' error, which is related to thread-local storage.
# It also adds -lkernel32 to resolve GetThreadId.
exec x86_64-w64-mingw32-g++ "${final_args[@]}" -static-libgcc -Wl,-Bstatic -lstdc++ -ldbghelp -lgcc_eh -l:libpthread.a -lmsvcrt -lmingwex -lmingw32 -lgcc -lmsvcrt -lmingwex -lkernel32 -Wl,-Bdynamic