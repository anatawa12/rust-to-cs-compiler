/// Stubs for Rust core library functions referenced by transpiled code.
///
/// When the transpiler encounters calls to standard library methods such as
/// `str::as_bytes` or `<[T]>::is_empty`, it emits fully-qualified calls to
/// top-level static classes following the `global::mod_core.*` naming
/// convention.  This file provides implementations of those stubs so that
/// generated test code compiles and behaves correctly.
///
/// These stubs are intentionally placed in the compiler-tests project (not in
/// the runtime library) because they depend on BCL types (`string`, UTF-8
/// encoding) that are only available on net8.0, whereas the runtime targets
/// netstandard2.0.
using System;
using System.Runtime.InteropServices;
using r2CsRuntime;

/// <summary>Stubs for <c>core::str</c> functions.</summary>
public static class mod_core
{
    /// <summary>Stubs for <c>core::str</c> intrinsics.</summary>
    public static class mod_str
    {
        /// <summary>
        /// Returns a slice of the UTF-8 bytes of <paramref name="s"/>.
        /// Corresponds to Rust's <c>str::as_bytes(&amp;self) -&gt; &amp;[u8]</c>.
        ///
        /// The returned <see cref="LenRef{T}"/> uses a pinned GCHandle so the
        /// underlying byte array is not moved by the GC for the duration of the
        /// call chain.  This is only correct within a single synchronous call
        /// and is sufficient for the transpiled test cases.
        /// </summary>
        public static unsafe LenRef<Slice<byte>> m_as_bytes(string s)
        {
            if (s.Length == 0)
                return new LenRef<Slice<byte>>(0, 0);

            byte[] utf8 = System.Text.Encoding.UTF8.GetBytes(s);

            // Pin the array so its address is stable for the lifetime of the
            // returned LenRef.  For testing purposes the handle is intentionally
            // leaked (one handle per call); the amount is negligible.
            GCHandle handle = GCHandle.Alloc(utf8, GCHandleType.Pinned);
            nint ptr = handle.AddrOfPinnedObject();
            return new LenRef<Slice<byte>>(ptr, utf8.Length);
        }
    }

    /// <summary>Stubs for <c>core::slice</c> intrinsics.</summary>
    public static class mod_slice
    {
        /// <summary>
        /// Returns <c>true</c> when the slice has zero elements.
        /// Corresponds to Rust's <c>&lt;[T]&gt;::is_empty(&amp;self) -&gt; bool</c>.
        /// </summary>
        public static bool m_is_empty<T>(LenRef<Slice<T>> slice) =>
            slice.Length == 0;
    }

    /// <summary>Stubs for <c>core::num</c> (integer wrapping/checked/saturating ops).</summary>
    public static class mod_num
    {
        // i32
        public static int  m_wrapping_add(int a, int b)  => unchecked(a + b);
        public static int  m_wrapping_sub(int a, int b)  => unchecked(a - b);
        public static int  m_wrapping_mul(int a, int b)  => unchecked(a * b);
        public static int  m_wrapping_neg(int a)         => unchecked(-a);
        public static int  m_wrapping_shl(int a, int b)  => unchecked(a << b);
        public static int  m_wrapping_shr(int a, int b)  => unchecked(a >> b);
        public static int  m_saturating_add(int a, int b) {
            long r = (long)a + b;
            if (r > int.MaxValue) return int.MaxValue;
            if (r < int.MinValue) return int.MinValue;
            return (int)r;
        }
        public static int  m_saturating_sub(int a, int b) {
            long r = (long)a - b;
            if (r > int.MaxValue) return int.MaxValue;
            if (r < int.MinValue) return int.MinValue;
            return (int)r;
        }
        // u32
        public static uint m_wrapping_add(uint a, uint b) => unchecked(a + b);
        public static uint m_wrapping_sub(uint a, uint b) => unchecked(a - b);
        public static uint m_wrapping_mul(uint a, uint b) => unchecked(a * b);
        // i64
        public static long m_wrapping_add(long a, long b) => unchecked(a + b);
        public static long m_wrapping_sub(long a, long b) => unchecked(a - b);
        public static long m_wrapping_mul(long a, long b) => unchecked(a * b);
        // u64
        public static ulong m_wrapping_add(ulong a, ulong b) => unchecked(a + b);
        public static ulong m_wrapping_sub(ulong a, ulong b) => unchecked(a - b);
        public static ulong m_wrapping_mul(ulong a, ulong b) => unchecked(a * b);
    }
}
