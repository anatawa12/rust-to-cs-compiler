using System;

namespace r2CsRuntime;

/// <summary>
/// Low-level intrinsic helpers used by transpiled Rust code.
/// These correspond to MIR operations that have no direct C# equivalent.
/// </summary>
public static class Intrinsics
{
    /// <summary>
    /// Creates an array of length <paramref name="count"/> where every element equals <paramref name="value"/>.
    /// Corresponds to the Rust <c>[value; count]</c> repeat expression.
    /// </summary>
    public static T[] Repeat<T>(T value, ulong count)
    {
        var arr = new T[(int)count];
        for (int i = 0; i < arr.Length; i++)
            arr[i] = value;
        return arr;
    }

    /// <summary>
    /// Three-way comparison (spaceship operator), returning -1, 0, or 1.
    /// Corresponds to the Rust <c>cmp</c> BinOp.
    /// </summary>
    public static sbyte Cmp<T>(T a, T b) where T : IComparable<T>
    {
        int c = a.CompareTo(b);
        return c < 0 ? (sbyte)-1 : c > 0 ? (sbyte)1 : (sbyte)0;
    }
}
