using System.Runtime.CompilerServices;

namespace r2CsRuntime.Internal;

/// <summary>
/// Internal CLR-layout helper used to obtain a managed <c>ref byte</c> to the
/// first data byte of any managed object.
///
/// <para><strong>How it works</strong></para>
/// Every .NET managed object has the following in-memory layout:
/// <code>
/// [Method Table pointer — IntPtr.Size bytes]
/// [Instance field 0    — offset 0 from data start]
/// [Instance field 1    — ...]
/// </code>
/// <c>Unsafe.As&lt;RawData&gt;(obj)</c> reinterprets <paramref name="obj"/>'s
/// managed reference as a <see cref="RawData"/> reference without any type
/// checks. Because <see cref="Data"/> is the only (and therefore first)
/// instance field of <see cref="RawData"/>, a managed reference to
/// <c>Unsafe.As&lt;RawData&gt;(obj).Data</c> is a <c>ref byte</c> that points
/// to offset 0 of <paramref name="obj"/>'s instance data — i.e., the same
/// location as a reference to <paramref name="obj"/>'s own first field.
///
/// <para>This pattern is used throughout the .NET runtime itself (e.g.
/// <c>MemoryMarshal.GetArrayDataReference</c>) and is stable across CLR
/// implementations, even though it is not formally documented as a public
/// contract.</para>
///
/// <para>The byte offset stored in <see cref="Ref{T}"/>, <see cref="LenRef{T}"/>,
/// etc. is always relative to this data-start reference, so it remains valid
/// even after the GC moves the object (the method-table prefix is not
/// included in the offset).</para>
/// </summary>
internal sealed class RawData
{
#pragma warning disable CS0649 // Field is never assigned — accessed only via Unsafe
    public byte Data;
#pragma warning restore CS0649

    /// <summary>
    /// Returns a managed <c>ref byte</c> to the first data byte of
    /// <paramref name="obj"/>, suitable for use with
    /// <see cref="Unsafe.AddByteOffset{T}"/> and
    /// <see cref="Unsafe.ByteOffset{T}"/>.
    /// </summary>
    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    internal static ref byte GetDataRef(object obj)
        => ref Unsafe.As<RawData>(obj)!.Data;
}
