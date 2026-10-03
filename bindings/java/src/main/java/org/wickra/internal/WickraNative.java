package org.wickra.internal;

import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodType;
import java.lang.ref.Cleaner;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Native library resolution and FFM downcall plumbing for the Wickra C ABI.
 *
 * <p>The native library is located in one of two ways. When the binding is
 * consumed as a packaged jar the per-platform library ships under
 * {@code /native/<os>-<arch>/} and is extracted to a temporary file at load
 * time. For local development (running against a {@code cargo build}) the
 * resolver walks up the directory tree to find {@code target/release} or
 * {@code target/debug}. Every candidate is validated to actually export the
 * Wickra ABI before it is accepted, and one that fails is skipped rather than
 * aborting the load, so an unrelated library of the same name can neither
 * shadow the real one nor hide it.
 *
 * <p>This is internal plumbing; application code uses the generated indicator
 * classes in {@code org.wickra}.
 */
public final class WickraNative {
    private WickraNative() {
    }

    /** Any exported symbol works as a fingerprint; sma_new exists in every build. */
    private static final String SENTINEL = "wickra_sma_new";

    static final Cleaner CLEANER = Cleaner.create();
    private static final Linker LINKER = Linker.nativeLinker();
    // The global arena, not a shared one: the library is never unloaded either
    // way, but a downcall into a library of a closeable arena acquires and
    // releases that arena around every call to keep it loaded -- 6 of the 9 ns
    // a streaming update took.
    private static final Arena LIB_ARENA = Arena.global();
    private static final SymbolLookup LOOKUP = loadLibrary();

    /** Build a downcall handle for one C function. Internal use by the generated code. */
    public static MethodHandle downcall(String name, FunctionDescriptor descriptor) {
        MemorySegment symbol = LOOKUP.find(name)
                .orElseThrow(() -> new UnsatisfiedLinkError("wickra: missing symbol " + name));
        return LINKER.downcallHandle(symbol, descriptor);
    }

    /**
     * A downcall handle for a per-tick {@code _update} function, linked as
     * critical: no thread-state transition around the call, which halves its
     * cost. Valid because an update is short, never calls back into Java and is
     * passed only native memory; the garbage collector waits for it to return.
     */
    public static MethodHandle downcallCritical(String name, FunctionDescriptor descriptor) {
        MemorySegment symbol = LOOKUP.find(name)
                .orElseThrow(() -> new UnsatisfiedLinkError("wickra: missing symbol " + name));
        return LINKER.downcallHandle(symbol, descriptor, Linker.Option.critical(false));
    }

    private static final ConcurrentHashMap<String, MethodHandle> HEAP_DOWNCALLS = new ConcurrentHashMap<>();

    /**
     * The batch function {@code name} linked to take heap memory: Java arrays
     * passed in place through {@link MemorySegment#ofArray}, with no copy into
     * native memory and none back. Linked critical with heap access -- the
     * garbage collector waits for the call, which keeps the arrays where they
     * are -- once per function, on first use, from the signature of its
     * ordinary handle {@code regular}. Valid because a batch never calls back
     * into Java.
     */
    public static MethodHandle heapDowncall(String name, MethodHandle regular) {
        return HEAP_DOWNCALLS.computeIfAbsent(name, symbolName -> {
            MethodType type = regular.type();
            MemoryLayout[] params = type.parameterList().stream()
                    .map(WickraNative::layoutOf)
                    .toArray(MemoryLayout[]::new);
            FunctionDescriptor descriptor = type.returnType() == void.class
                    ? FunctionDescriptor.ofVoid(params)
                    : FunctionDescriptor.of(layoutOf(type.returnType()), params);
            MemorySegment symbol = LOOKUP.find(symbolName)
                    .orElseThrow(() -> new UnsatisfiedLinkError("wickra: missing symbol " + symbolName));
            return LINKER.downcallHandle(symbol, descriptor, Linker.Option.critical(true));
        });
    }

    /** The C layout a downcall carries a Java parameter or result type as. */
    private static MemoryLayout layoutOf(Class<?> carrier) {
        if (carrier == MemorySegment.class) {
            return ValueLayout.ADDRESS;
        }
        if (carrier == long.class) {
            return ValueLayout.JAVA_LONG;
        }
        if (carrier == double.class) {
            return ValueLayout.JAVA_DOUBLE;
        }
        if (carrier == int.class) {
            return ValueLayout.JAVA_INT;
        }
        if (carrier == byte.class) {
            return ValueLayout.JAVA_BYTE;
        }
        throw new IllegalArgumentException("wickra: no C layout for " + carrier);
    }

    /**
     * Register an opaque handle for release via its {@code _free} function when the
     * owning wrapper becomes unreachable (or is closed). The action holds no
     * reference to the owner, so it never keeps it alive.
     */
    public static Cleaner.Cleanable register(Object owner, MemorySegment handle, MethodHandle free) {
        return CLEANER.register(owner, new FreeAction(handle, free));
    }

    /**
     * Allocate a C {@code bool*} buffer (one byte per element) from flag values.
     * The C ABI takes the cross-section state flags as {@code const bool*}, so
     * they must be one byte each rather than eight-byte doubles.
     */
    public static MemorySegment boolSegment(Arena arena, boolean[] flags) {
        byte[] bytes = new byte[flags.length];
        for (int i = 0; i < flags.length; i++) {
            bytes[i] = (byte) (flags[i] ? 1 : 0);
        }
        return arena.allocateFrom(java.lang.foreign.ValueLayout.JAVA_BYTE, bytes);
    }

    /**
     * Check a caller segment handed straight to a native batch: it must be
     * native (off-heap) memory, hold exactly {@code n} elements of
     * {@code layout}, and be aligned for that element type, since the native
     * side reads it as a typed slice without copying.
     */
    public static void checkBatchSegment(MemorySegment segment, ValueLayout layout, long n) {
        if (!segment.isNative()) {
            throw new IllegalArgumentException("wickra: batch segments must be native (off-heap) memory");
        }
        if (segment.byteSize() != n * layout.byteSize()) {
            throw new IllegalArgumentException("wickra: every batch segment must hold the same number of elements");
        }
        if (segment.address() % layout.byteAlignment() != 0) {
            throw new IllegalArgumentException("wickra: batch segment is not aligned for its element type");
        }
    }

    /** Re-throw a {@link MethodHandle#invokeExact} {@link Throwable} as an unchecked exception. */
    public static RuntimeException rethrow(Throwable t) {
        if (t instanceof RuntimeException re) {
            return re;
        }
        if (t instanceof Error e) {
            throw e;
        }
        return new RuntimeException(t);
    }

    private record FreeAction(MemorySegment handle, MethodHandle free) implements Runnable {
        @Override
        public void run() {
            try {
                free.invokeExact(handle);
            } catch (Throwable ignored) {
                // Best-effort release during finalization; nothing actionable here.
            }
        }
    }

    private static SymbolLookup loadLibrary() {
        List<String> rejected = new ArrayList<>();
        for (Path lib : locate()) {
            SymbolLookup lookup;
            try {
                lookup = SymbolLookup.libraryLookup(lib, LIB_ARENA);
            } catch (IllegalArgumentException e) {
                rejected.add(lib + " (" + e.getMessage() + ")");
                continue;
            }
            if (lookup.find(SENTINEL).isPresent()) {
                return lookup;
            }
            // A file of the right name that is not our library -- keep looking
            // rather than aborting, which is what a stale build or an unrelated
            // library earlier in the search order used to do. It stays loaded
            // until the process exits; there is no unload in the FFM API, and
            // the alternative would be to dlopen it a second time to accept it.
            rejected.add(lib + " (does not export the C ABI)");
        }
        throw new UnsatisfiedLinkError(
                "wickra: could not load the native library (" + libraryFileName()
                        + "). Bundle it under resources/native/" + platformDir()
                        + "/ or build the C ABI with `cargo build -p wickra-c --release`."
                        + (rejected.isEmpty() ? "" : " Rejected: " + String.join("; ", rejected)));
    }

    /**
     * Every place the library might be, in the order they should be tried: the
     * bundled copy first, then each {@code target/release} or {@code target/debug}
     * found by walking up from the working directory and from this class's own
     * location.
     */
    private static List<Path> locate() {
        List<Path> candidates = new ArrayList<>();
        Path bundled = extractBundled();
        if (bundled != null) {
            candidates.add(bundled);
        }
        candidates.addAll(findInCargoTarget());
        return candidates;
    }

    private static Path extractBundled() {
        String resource = "/native/" + platformDir() + "/" + libraryFileName();
        try (InputStream in = WickraNative.class.getResourceAsStream(resource)) {
            if (in == null) {
                return null;
            }
            Path tmp = Files.createTempFile("wickra-", "-" + libraryFileName());
            tmp.toFile().deleteOnExit();
            Files.copy(in, tmp, StandardCopyOption.REPLACE_EXISTING);
            return tmp;
        } catch (IOException e) {
            return null;
        }
    }

    private static List<Path> findInCargoTarget() {
        return findInCargoTarget(
                new Path[] {Paths.get(System.getProperty("user.dir", ".")), codeSourceDir()});
    }

    /**
     * Walk up from each base looking for {@code target/<profile>/<library>},
     * collecting every hit rather than stopping at the first. Package-private so
     * the resolution test can point it at a synthetic tree.
     */
    static List<Path> findInCargoTarget(Path[] bases) {
        String file = libraryFileName();
        List<Path> found = new ArrayList<>();
        for (Path base : bases) {
            Path dir = base;
            for (int i = 0; i < 16 && dir != null; i++) {
                for (String profile : new String[] {"release", "debug"}) {
                    Path candidate = dir.resolve("target").resolve(profile).resolve(file);
                    if (Files.isRegularFile(candidate) && !found.contains(candidate)) {
                        found.add(candidate);
                    }
                }
                dir = dir.getParent();
            }
        }
        return found;
    }

    private static Path codeSourceDir() {
        try {
            Path p = Paths.get(WickraNative.class.getProtectionDomain()
                    .getCodeSource().getLocation().toURI());
            return Files.isDirectory(p) ? p : p.getParent();
        } catch (Exception e) {
            return null;
        }
    }

    private static String osName() {
        String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        if (os.contains("win")) {
            return "win";
        }
        if (os.contains("mac") || os.contains("darwin")) {
            return "osx";
        }
        return "linux";
    }

    private static String archName() {
        String arch = System.getProperty("os.arch", "").toLowerCase(Locale.ROOT);
        if (arch.equals("aarch64") || arch.equals("arm64")) {
            return "arm64";
        }
        return "x64";
    }

    private static String platformDir() {
        return osName() + "-" + archName();
    }

    private static String libraryFileName() {
        return switch (osName()) {
            case "win" -> "wickra.dll";
            case "osx" -> "libwickra.dylib";
            default -> "libwickra.so";
        };
    }
}
