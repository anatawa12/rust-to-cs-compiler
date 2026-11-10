#nullable enable
using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Net.Mail;
using System.Runtime.CompilerServices;
using System.Text;
using System.Web;
using System.Xml.Schema;
using r2CsRuntime;

namespace VrcGetVpm;

public static partial class crt_Core
{
    public static partial class mod_Fmt
    {
        public partial class s_Formatter
        {
            public mod_Result.s_Result<ValueTuple, s_Error> m_WriteChar(Rune c) => m_WriteStr(c.EncodeUtf8());
            public mod_Result.s_Result<ValueTuple, s_Error> m_WriteFmt(crt_Alloc.mod_String.s_String value) => m_WriteStr(value.m_AsStr());
        }

        public interface t_Display
        {
            public global::VrcGetVpm.crt_Core.mod_Result.s_Result<global::System.ValueTuple,
                    global::VrcGetVpm.crt_Core.mod_Fmt.s_Error>
                DisplayFmt(global::VrcGetVpm.crt_Core.mod_Fmt.s_Formatter p_0);
        }
    }

    public static partial class mod_Option
    {
        public sealed partial class s_Option<P_T> : IEnumerable<P_T>
        {
            public s_Option<P_T> m_Take()
            {
                switch (this.inner)
                {
                    case v_Some some:
                    {
                        inner = v_None.instance;
                        return some;
                    }
                    default: return this;
                }
            }
            
            public P_T m_GetOrInsertWith(Func<P_T> inner)
            {
                switch (this)
                {
                    case v_Some ok:
                        return ok.f_0;
                    case v_None _:
                        var value = inner();
                        set(v_Some.ctor(value));
                        return value;
                    case null:
                        throw new NullReferenceException();
                }
            }

#if SERDE
            public static global::VrcGetVpm.crt_Core.mod_Result.s_Result<s_Option<T>, P_D_Error /* <D as Deserializer<'de>>::Error */> m_Deserialize<P_D_Error, P_D_Error_statics>(global::VrcGetVpm.crt_SerdeCore.mod_De.t_Deserializer<P_D_Error, P_D_Error_statics> l_deserializer_0)
                where P_D_Error : crt_SerdeCore.mod_De.t_Error<P_D_Error, P_D_Error_statics>, crt_Core.mod_Error.t_Error, crt_Core.mod_Fmt.t_Display, crt_Core.mod_Fmt.t_Debug
                where P_D_Error_statics : struct, crt_SerdeCore.mod_De.t_Error<P_D_Error, P_D_Error_statics>.Statics
            {
                throw new NotImplementedException();
            }
#endif

            public IEnumerator<P_T> GetEnumerator()
            {
                if (this is v_Some some) yield return some.f_0;
            }

            IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
        }
    }

    public static partial class mod_Iter
    {
        public static partial class mod_Sources
        {
            public static partial class mod_FromFn
            {
                public static IEnumerator<T> m_FromFn<T>(Func<mod_Option.s_Option<T>> func)
                {
                    while (true)
                    {
                        if (func() is mod_Option.s_Option<T>.v_Some { f_0: var result })
                            yield return result;
                        else
                            break;
                    }
                }
            }
        }
    }

    public static partial class mod_Result
    {
        public sealed partial class s_Result<P_T, P_E>
        {
            public P_T m_UnwrapOrElse(Func<P_T> fallback)
            {
                switch (this)
                {
                    case v_Ok ok:
                        return ok.f_0;
                    case v_Err _:
                        return fallback();
                    case null:
                        throw new NullReferenceException();
                }
            }
        }
    }

    public static partial class mod_Marker
    {
        public class s_PhantomData<T>
        {
            public static s_PhantomData<T> instance = new();
            public static s_PhantomData<T> m_Default() => instance;
        }
    }

    public static partial class mod_Cmp
    {
        public partial class s_Ordering
        {
            public int CsOrder() => this switch
            {
                v_Equal => 0,
                v_Greater => 1,
                v_Less => -1,
                _ => throw new InvalidOperationException($"Bad Value: {this}"),
            };
        }
    }
}

public static partial class crt_Core
{
    public static class mod_Num
    {
        public static class mod_Error
        {
            public class s_ParseIntError
            {
                public static readonly s_ParseIntError instance = new s_ParseIntError();
            }
        }

        public static class mod_Nonzero
        {
            public class s_NonZero<T, A_NonZero>
            {
                public required A_NonZero inner;
            }
        }

        public static class mod_NicheTypes
        {
            public class s_NonZeroU8Inner
            {
                public required byte inner;
            }
        }
    }
}

public static class NonZeroExt
{
    extension(crt_Core.mod_Num.mod_Nonzero.s_NonZero<byte, crt_Core.mod_Num.mod_NicheTypes.s_NonZeroU8Inner> self)
    {
        public static crt_Core.mod_Result.s_Result<crt_Core.mod_Num.mod_Nonzero.s_NonZero<byte, crt_Core.mod_Num.mod_NicheTypes.s_NonZeroU8Inner>,
            crt_Core.mod_Num.mod_Error.s_ParseIntError> m_FromStr(RustStr data) => m_FromStr(data.ToString());
        public static crt_Core.mod_Result.s_Result<crt_Core.mod_Num.mod_Nonzero.s_NonZero<byte, crt_Core.mod_Num.mod_NicheTypes.s_NonZeroU8Inner>,
                crt_Core.mod_Num.mod_Error.s_ParseIntError> m_FromStr(string data)
        {
            if (byte.TryParse(data, out byte result) && result != 0)
            {
                return crt_Core.mod_Result.s_Result<crt_Core.mod_Num.mod_Nonzero.s_NonZero<byte,
                        crt_Core.mod_Num.mod_NicheTypes.s_NonZeroU8Inner>,
                    crt_Core.mod_Num.mod_Error.s_ParseIntError>.v_Ok.ctor(
                    new crt_Core.mod_Num.mod_Nonzero.s_NonZero<byte,
                        crt_Core.mod_Num.mod_NicheTypes.s_NonZeroU8Inner>
                    {
                        inner = new crt_Core.mod_Num.mod_NicheTypes.s_NonZeroU8Inner
                        {
                            inner = result
                        }
                    }
                );
                ;
            }
            else
            {
                return crt_Core.mod_Result.s_Result<crt_Core.mod_Num.mod_Nonzero.s_NonZero<byte,
                        crt_Core.mod_Num.mod_NicheTypes.s_NonZeroU8Inner>,
                    crt_Core.mod_Num.mod_Error.s_ParseIntError>.v_Err.ctor(
                    crt_Core.mod_Num.mod_Error.s_ParseIntError.instance
                );
            }
        }

        public byte m_Get() => self.inner.inner;
    }
}

public static partial class crt_Core
{
    public static class mod_Hash
    {
        public interface t_Hash
        {
            public void m_Hash(crt_Core.mod_Hash.t_Hasher l_state_0);

            public interface Statics
            {
            }
        }

        public interface t_Hasher
        {
        }
    }

    public static class mod_Sync
    {
        public static class mod_Atomic
        {
            public class s_Atomic<T, Error>
            {
                private bool m_value;
                private s_Atomic(bool value) => m_value = value;
                public static s_Atomic<bool, crt_Core.mod_Sync.mod_Atomic.mod_Private.s_Align1<byte>> m_New(bool value) => new(value);
                public bool m_Load(s_Ordering ordering) => Volatile.Read(ref m_value);
                public void m_Store(bool value, s_Ordering ordering) => Volatile.Write(ref m_value, value);
            }

            public static partial class mod_Private
            {
                public class s_Align1<T>
                {
                    
                }
            }

            public class s_AtomicBool
            {
                private bool m_value;
                private s_AtomicBool(bool value) => m_value = value;

                public static s_AtomicBool m_New(bool value) => new s_AtomicBool(value);

                public bool m_Load(s_Ordering ordering) => Volatile.Read(ref m_value);
                public void m_Store(bool value, s_Ordering ordering) => Volatile.Write(ref m_value, value);
            }

            [Union]
            public class s_Ordering
            {
                private object m_value;
                public object Value => m_value;

                public s_Ordering(v_Relaxed value) => m_value = value;
                public sealed class v_Relaxed
                {
                    public static s_Ordering instance = new v_Relaxed();
                }
            }
        }
    }

    // TODO: this is async internal. remove them
    public static class mod_Task
    {
        public static class mod_Wake
        {
            public class s_Context
            {
            }
        }

        public static class mod_Poll
        {
            [Union]
            public class s_Poll<T>
            {
                private object _value;
                public object Value => _value;

                public s_Poll(v_Ready value) => _value = value;
                public class v_Ready
                {
                    public required T f_0;
                    public static s_Poll<T> ctor(T f_0) => new v_Ready { f_0 = f_0 };
                }

                public s_Poll(v_Pending value) => _value = value;
                public class v_Pending
                {
                    public readonly static s_Poll<T> instance = new v_Pending {};
                }
            }
        }
    }

    public static class mod_Str
    {
        public static class mod_Traits
        {
            public interface t_FromStr<T_Self, T_SelfStatics, A_Error>
                where T_Self: t_FromStr<T_Self, T_SelfStatics, A_Error>
                where T_SelfStatics: struct, t_FromStr<T_Self, T_SelfStatics, A_Error>.Statics
            {
                public interface Statics
                {
                    public crt_Core.mod_Result.s_Result<T_Self, A_Error> m_FromStr(RustStr l_s_0);
                }
            }
        }
    }

    public static class mod_Ops
    {
        public static class mod_Deref
        {
            public interface t_Deref<P_T>
            {
                public P_T m_Deref();
            }
        }
    }

    public static class mod_Future
    {
        public static partial class mod_Ready
        {
            public static async RustTask<T> m_Ready<T>(T value) => value;
        }
    }

    public static partial class mod_Io
    {
        public static partial class mod_Error
        {
            public partial class s_Error : crt_Core.mod_Error.t_Error
            {
                public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DebugFmt(crt_Core.mod_Fmt.s_Formatter p_0)
                {
                    return p_0.m_WriteStr(this.f_message.m_AsStr());
                }

                public static s_Error m_From(crt_SerdeJson.mod_Error.s_Error lValue0) =>
                    m_New(s_ErrorKind.v_InvalidData.instance, lValue0);
            }
        }
    }

    public static class mod_Mem
    {
        public static void m_Drop(object drop) => throw null;
        public static T m_Replace<T>(T slot, T value) => throw null;

        public static T m_Take<T, T_Statics>(T fUserRepos)
            //where T_Statics : struct, mod_Default.t_Default<T, T_Statics>.Statics 
            => throw null;
    }
}

public static class DebugDisplayExt
{
    extension(crt_Core.mod_Fmt.t_Debug self)
    {
        public crt_Alloc.mod_String.s_String DebugStr() => FormatterToString(self.DebugFmt);
    }
    
    extension(crt_Core.mod_Fmt.t_Display self)
    {
        public crt_Alloc.mod_String.s_String m_ToString() => FormatterToString(self.DisplayFmt);
    }

    private static crt_Alloc.mod_String.s_String FormatterToString(Func<crt_Core.mod_Fmt.s_Formatter, crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error>> fmt)
    {
        var builder = crt_Alloc.mod_String.s_String.m_New();
        var result = fmt(new crt_Core.mod_Fmt.s_Formatter { f_write = builder });
        if (!result.m_IsOk()) throw new PanicException("Formatting");
        return builder;
    }
}

public static class ArrayExt
{
    extension<T>(T[] self)
    {
        public Slice<T> m_Deref() => self;
    }
}

public static class StringExt
{
    private static UTF8Encoding UTF8 = new UTF8Encoding(false);

    extension(string self)
    {
        public static string m_From(string _) => _;
        public static string m_From(RustStr str) => str.ToString();
        public static string m_New(RustStr str) => str.ToString();
        public static string m_WithCapacity(nuint cap) => "";
        public string m_AsOsStr() => self;


        public nuint m_Len() => (nuint)UTF8.GetByteCount(self);
        public RustStr m_ToStringLossy() => ((crt_Alloc.mod_String.s_String)self).m_AsStr(); // Since Cow is mapped to str
        public string m_ToString() => self;
        public string m_Clone() => self;
        public bool m_EndsWith(RustStr other) => self.EndsWith(other.ToString(), StringComparison.Ordinal);
        public bool m_StartsWith(RustStr other) => self.StartsWith(other.ToString(), StringComparison.Ordinal);

        public string DebugStr() => self;
    }
}
