#nullable enable
using System;
using System.Collections;
using System.Collections.Generic;
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
        public interface s_Error
        {
            
        }

        public class s_Formatter
        {
            private StringBuilder _builder;

            public s_Formatter(StringBuilder? builder = null)
            {
                _builder = builder ?? new StringBuilder();
            }

            public s_DebugStruct m_DebugStruct(string typeName) => new(_builder, typeName);

            public mod_Result.s_Result<ValueTuple, s_Error> m_WriteChar(char c)
            {
                _builder.Append(c);
                return mod_Result.s_Result<ValueTuple, s_Error>.v_Ok.ctor(default);
            }

            public mod_Result.s_Result<ValueTuple, s_Error> m_WriteStr(string value)
            {
                _builder.Append(value);
                return mod_Result.s_Result<ValueTuple, s_Error>.v_Ok.ctor(default);
            }
            public mod_Result.s_Result<ValueTuple, s_Error> m_WriteFmt(string value) => m_WriteStr(value);
        }

        public class s_DebugStruct
        {
            bool first = true;
            private readonly StringBuilder _builder;

            public s_DebugStruct(StringBuilder builder, string typeName)
            {
                _builder = builder;
                builder.Append(typeName).Append(" {");
            }

            public s_DebugStruct m_Field(string field, t_Debug debug)
            {
                if (!first) _builder.Append(", ");
                first = false;
                _builder.Append(field).Append(": ");
                debug.DebugFmt(new s_Formatter(_builder));
                return this;
            }

            public mod_Result.s_Result<ValueTuple, s_Error> m_Finish()
            {
                _builder.Append("}");
                return mod_Result.s_Result<ValueTuple, s_Error>.v_Ok.ctor(default);
            }
        }

        public interface t_Debug
        {
            public global::VrcGetVpm.crt_Core.mod_Result.s_Result<global::System.ValueTuple,
                    global::VrcGetVpm.crt_Core.mod_Fmt.s_Error>
                DebugFmt(global::VrcGetVpm.crt_Core.mod_Fmt.s_Formatter p_0);
        }

        public interface t_Display
        {
            public global::VrcGetVpm.crt_Core.mod_Result.s_Result<global::System.ValueTuple,
                    global::VrcGetVpm.crt_Core.mod_Fmt.s_Error>
                DisplayFmt(global::VrcGetVpm.crt_Core.mod_Fmt.s_Formatter p_0);
        }

        public interface t_Write
        {
        }
    }

    public static partial class mod_Option
    {
        [Union]
        public sealed class s_Option<T> : mod_Default.t_Default<s_Option<T>, s_Option<T>.Statics>, IEnumerable<T>
        {
            private object inner;
            public object Value => inner;
            public void set(s_Option<T> o) => inner = o.inner;
            public s_Option(v_Some value) => inner = value;
            public s_Option(v_None value) => inner = value;

            public class v_Some
            {
                public T f_0;

                public static s_Option<T> ctor(T f0) => new v_Some() { f_0 = f0 };
            }

            public class v_None
            {
                public static s_Option<T> instance = new v_None();

                private v_None()
                {
                }
            }

            public s_Option<T> m_AsMut() => this;
            public s_Option<T> m_AsRef() => this;
            public s_Option<T> m_AsDeref() => this;
            public s_Option<T> m_Cloned() => this;
            public static s_Option<T> m_Default() => v_None.instance;

            public bool m_IsNone() => this is v_None;
            public bool m_IsSome() => this is v_Some;
            public T m_Unwrap() => this is v_Some some ? some.f_0 : throw new NullReferenceException();

            public s_Option<T> m_Take()
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

            public mod_Result.s_Result<T, E> m_OkOr<E>(E e) => this switch
            {
                v_Some ok => mod_Result.s_Result<T, E>.v_Ok.ctor(ok.f_0),
                v_None _ => mod_Result.s_Result<T, E>.v_Err.ctor(e),
                null => throw new NullReferenceException(),
            };
            public mod_Result.s_Result<T, E> m_OkOrElse<E>(Func<E> e) => this switch
            {
                v_Some ok => mod_Result.s_Result<T, E>.v_Ok.ctor(ok.f_0),
                v_None _ => mod_Result.s_Result<T, E>.v_Err.ctor(e()),
                null => throw new NullReferenceException(),
            };

            public s_Option<R> m_AndThen<R>(Func<T, s_Option<R>> f) => this switch
            {
                v_Some ok => f(ok.f_0),
                v_None _ => s_Option<R>.v_None.instance,
                null => throw new NullReferenceException(),
            };
            
            public s_Option<R> m_Map<R>(Func<T, R> f) => this switch
            {
                v_Some ok => s_Option<R>.v_Some.ctor(f(ok.f_0)),
                v_None _ => s_Option<R>.v_None.instance,
                null => throw new NullReferenceException(),
            };

            public s_Option<T> m_Or(s_Option<T> or) => this switch
            {
                v_Some ok => ok,
                v_None _ => or,
                null => throw new NullReferenceException(),
            };

            public s_Option<T> m_Filter(Func<T, bool> filter) => this switch
            {
                v_Some ok when filter(ok.f_0) => ok,
                _ => v_None.instance,
            };

            public T m_UnwrapOr(T or) => this switch
            {
                v_Some ok => ok.f_0,
                v_None _ => or,
                null => throw new NullReferenceException(),
            };

            public T m_UnwrapOrElse(Func<T> or) => this switch
            {
                v_Some ok => ok.f_0,
                v_None _ => or(),
                null => throw new NullReferenceException(),
            };

            public s_Option<T> m_OrElse(Func<s_Option<T>> func)=> this switch
            {
                v_Some ok => ok,
                v_None _ => func(),
                null => throw new NullReferenceException(),
            };

            public bool m_IsSomeAnd(Func<T, bool> func) => this switch
            {
                v_Some ok => func(ok.f_0),
                v_None _ => false,
                null => throw new NullReferenceException(),
            };
            
            public T m_GetOrInsertWith(Func<T> inner)
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

            public struct Statics : mod_Default.t_Default<s_Option<T>, Statics>.Statics
            {
                public s_Option<T> m_Default() => v_None.instance;
            }

            public IEnumerator<T> GetEnumerator()
            {
                throw new NotImplementedException();
            }

            IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
        }
    }

    public static partial class mod_Iter
    {
        public static partial class mod_Traits
        {
            public static partial class mod_Iterator
            {
                public interface t_Iterator<Item>
                {
                    mod_Option.s_Option<Item> m_Next();
                }
            }

            public static class mod_Collect
            {
                public interface t_IntoIterator<AItem>
                {
                    public mod_Iterator.t_Iterator<AItem> m_IntoIter();
                    //public static implicit operator t_IntoIterator<AItem>(List<AItem> list) => throw null;
                }

                public interface t_FromIterator<TSelf, TSelf_Statics, AItem>
                    where TSelf: t_FromIterator<TSelf, TSelf_Statics, AItem>
                    where TSelf_Statics: struct, t_FromIterator<TSelf, TSelf_Statics, AItem>.Statics
                {
                    public interface Statics
                    {
                        public crt_VrcGetVpm.mod_Utils.mod_Json.s_JsonArray m_FromIter(
                            System.Collections.Generic.IEnumerable<AItem> l_iter_0);
                    }
                }
            }
        }

        public static partial class mod_Adapters
        {
            public static partial class mod_Fuse
            {
                public class s_Fuse<P_I>
                {
                    
                }
            }
        }

        public static partial class mod_Sources
        {
            public static partial class mod_FromFn
            {
                public static IEnumerator<T> m_FromFn<T>(Func<mod_Option.s_Option<T>> func)
                {
                    // TODO: this does not present double move
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
        [Union]
        public sealed class s_Result<T, E>
        {
            private object inner;
            public object Value => inner;
            public void set(s_Result<T, E> o) => inner = o.inner;
            public s_Result(v_Ok value) => inner = value;
            public s_Result(v_Err value) => inner = value;

            public mod_Option.s_Option<T> m_Ok() => this switch
            {
                v_Ok v => mod_Option.s_Option<T>.v_Some.ctor(v.f_0),
                _ => mod_Option.s_Option<T>.v_None.instance
            };

            public mod_Option.s_Option<E> m_Err() => this switch
            {
                v_Err v => mod_Option.s_Option<E>.v_Some.ctor(v.f_0),
                _ => mod_Option.s_Option<E>.v_None.instance
            };

            public bool m_IsOk() => this is v_Ok _;

            public class v_Ok
            {
                public required T f_0;

                public static s_Result<T, E> ctor(T f0) => new v_Ok { f_0 = f0 };
            }
            
            public class v_Err
            {
                public required E f_0;

                public static s_Result<T, E> ctor(E f0) => new v_Err { f_0 = f0 };
            }

            public s_Result<U, E> m_Map<U>(Func<T, U> f)
            {
                switch (this)
                {
                    case v_Ok ok:
                        return s_Result<U, E>.v_Ok.ctor(f(ok.f_0));
                    case v_Err err:
                        return s_Result<U, E>.v_Err.ctor(err.f_0);
                    case null:
                        throw new NullReferenceException();
                }
            }

            public s_Result<T, U> m_MapErr<U>(Func<E, U> f)
            {
                switch (this)
                {
                    case v_Ok ok:
                        return s_Result<T, U>.v_Ok.ctor(ok.f_0);
                    case v_Err err:
                        return s_Result<T, U>.v_Err.ctor(f(err.f_0));
                    case null:
                        throw new NullReferenceException();
                }
            }

            public s_Result<U, E> m_AndThen<U>(Func<T, s_Result<U, E>> f)
            {
                switch (this)
                {
                    case v_Ok ok:
                        return f(ok.f_0);
                    case v_Err err:
                        return s_Result<U, E>.v_Err.ctor(err.f_0);
                    case null:
                        throw new NullReferenceException();
                }
            }

            public T m_UnwrapOr(T value)
            {
                switch (this)
                {
                    case v_Ok ok:
                        return ok.f_0;
                    case v_Err _:
                        return value;
                    case null:
                        throw new NullReferenceException();
                }
            }

            public T m_UnwrapOrElse(Func<T> fallback)
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

            public T m_Unwrap() => m_UnwrapOrElse(() => throw new PanicException($"Unwrap on value: {(inner as mod_Fmt.t_Debug)?.DebugStr() ?? "value not implements Debug"}"));
        }
    }

    public static partial class mod_Clone
    {
        public interface t_Clone<Self>
        {
            public Self m_Clone();
        }
    }

    public static partial class mod_Marker
    {
        public interface t_Send {}
        public interface t_Sync {}

        public interface t_Copy<Self> : mod_Clone.t_Clone<Self>
        {
        }

        public class s_PhantomData<T>
        {
            public static s_PhantomData<T> instance = new();
            public static s_PhantomData<T> m_Default() => instance;
        }

        public interface t_Unpin
        {
            
        }
    }

    public static partial class mod_Default
    {
        public interface t_Default<Self, SelfStatics>
            where SelfStatics : struct, t_Default<Self, SelfStatics>.Statics
        {
            // static members only
            public interface Statics
            {
                public Self m_Default();
            }
        }
    }

    public static partial class mod_Cmp
    {
        public interface t_PartialEq<TOther>
        {
            public bool m_Eq(TOther p_0);
        }

        public interface t_Eq<TSelf> : t_PartialEq<TSelf>
        {
        }

        public interface t_PartialOrd<Rhs> : t_PartialEq<Rhs>
        {
            public mod_Option.s_Option<s_Ordering> m_PartialCmp(Rhs other);
        }

        public interface t_Ord<TSelf> : t_PartialOrd<TSelf>
        {
            s_Ordering m_Cmp(TSelf p_0);
        }

        public class s_Ordering
        {
            public bool m_IsEq() => this is v_Equal;
            public bool m_IsNe() => this is not v_Equal;
            public bool m_IsLt() => this is v_Less;
            public bool m_IsLe() => this is v_Less or v_Equal;
            public bool m_IsGt() => this is v_Greater;
            public bool m_IsGe() => this is v_Greater or v_Equal;

            public s_Ordering m_ThenWith(Func<s_Ordering> other) => this switch
            {
                v_Equal => other(),
                _ => this,
            };

            public int CsOrder() => this switch
            {
                v_Equal => 0,
                v_Greater => 1,
                v_Less => -1,
                _ => throw new InvalidOperationException($"Bad Value: {this}"),
            };

            public class v_Equal : s_Ordering
            {
                public static v_Equal instance = new v_Equal();
            }

            public class v_Greater : s_Ordering
            {
                public static v_Greater instance = new v_Greater();
            }

            public class v_Less : s_Ordering
            {
                public static v_Less instance = new v_Less();
            }
        }
    }

    public static partial class mod_Error
    {
        public interface t_Error: mod_Fmt.t_Display, mod_Fmt.t_Debug
        {
        }
    }

    public static partial class mod_Convert
    {
        public interface t_From<TSelf, TSelf_Statics, T>
            where TSelf: t_From<TSelf, TSelf_Statics, T>
            where TSelf_Statics: struct, t_From<TSelf, TSelf_Statics, T>.Statics 
        {
            interface Statics
            {
                public TSelf m_From(T p_0);
            }
        }
        
        public interface t_TryFrom<TSelf, TSelfStatics, T, A_Error>
            where TSelf: t_TryFrom<TSelf, TSelfStatics, T, A_Error>
            where TSelfStatics: struct, t_TryFrom<TSelf, TSelfStatics, T, A_Error>.Statics
        {
            interface Statics
            {
                public mod_Result.s_Result<TSelf, A_Error> m_TryFrom(T p_0);
            }
        }

        public interface t_Into<TOut>
        {
            
        }

        public interface t_AsRef<TOut>
        {
            public TOut m_AsRef();
        }

        public class s_Infallible
        {
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
                    public crt_Core.mod_Result.s_Result<T_Self, A_Error> m_FromStr(string l_s_0);
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

    public static class mod_Borrow
    {
        public interface t_Borrow<T>
        {
            public T m_Borrow();
        }
    }

    public static class mod_Future
    {
        public static partial class mod_Ready
        {
            public static async RustTask<T> m_Ready<T>(T value) => value;
        }
    }

    public static class mod_Io
    {
        public static class mod_Error
        {
            public class s_Error : crt_Core.mod_Error.t_Error
            {
                public static s_Error m_New<T>(s_ErrorKind error, T t) => throw null;

                public s_ErrorKind m_Kind() => throw null;

                public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DisplayFmt(
                    crt_Core.mod_Fmt.s_Formatter f) => throw null;

                public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DebugFmt(crt_Core.mod_Fmt.s_Formatter p_0)
                {
                    throw null;
                }

                public static s_Error m_From(crt_SerdeJson.mod_Error.s_Error lValue0) => throw null;
            }

            [Union]
            public class s_ErrorKind
            {
                private object m_value;
                public object Value => m_value;

                public s_ErrorKind(v_Interrupted value) => m_value = value;
                public class v_Interrupted
                {
                    public static s_ErrorKind instance = new v_Interrupted();
                }
                public s_ErrorKind(v_InvalidInput value) => m_value = value;
                public class v_InvalidInput
                {
                    public static s_ErrorKind instance = new v_InvalidInput();
                }
                public s_ErrorKind(v_InvalidData value) => m_value = value;
                public class v_InvalidData
                {
                    public static s_ErrorKind instance = new v_InvalidData();
                }
                public s_ErrorKind(v_UnexpectedEof value) => m_value = value;
                public class v_UnexpectedEof
                {
                    public static s_ErrorKind instance = new v_UnexpectedEof();
                }
                public s_ErrorKind(v_NotFound value) => m_value = value;
                public class v_NotFound
                {
                    public static s_ErrorKind instance = new v_NotFound();
                }
                public s_ErrorKind(v_AlreadyExists value) => m_value = value;
                public class v_AlreadyExists
                {
                    public static s_ErrorKind instance = new v_AlreadyExists();
                }
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
        public string DebugStr() => FormatterToString(self.DebugFmt);
    }
    
    extension(crt_Core.mod_Fmt.t_Display self)
    {
        public string m_ToString() => FormatterToString(self.DisplayFmt);
    }

    private static string FormatterToString(Func<crt_Core.mod_Fmt.s_Formatter, crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error>> fmt)
    {
        var builder = new StringBuilder();
        var result = fmt(new crt_Core.mod_Fmt.s_Formatter(builder));
        if (!result.m_IsOk()) throw new PanicException("Formatting");
        return builder.ToString();
    }
}
