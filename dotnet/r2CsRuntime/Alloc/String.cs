using System.Text;

namespace VrcGetVpm;

public static partial class crt_Alloc
{
    public static class mod_String
    {
        public class s_String : crt_Core.mod_Fmt.t_Display, crt_Core.mod_Fmt.t_Debug, crt_Core.mod_Cmp.t_Ord<RustStr>, crt_Core.mod_Cmp.t_Ord<s_String>
        {
            private mod_Vec.s_Vec<byte> _base;

            public static s_String m_New() => new s_String(mod_Vec.s_Vec<byte>.m_New());

            internal s_String(mod_Vec.s_Vec<byte> data) => _base = data;

            public RustStr m_AsStr() => RustStr.FromUtf8Unsafe(_base.m_AsSlice()); // TODO: optimize

            public void m_Clear() => _base.m_Clear();

            public static s_String m_Default() => m_New();

            public static explicit operator s_String(string cast) => new s_String(new mod_Vec.s_Vec<byte>(Encoding.UTF8.GetBytes(cast)));

            public RustStr m_IntoBoxedStr() => m_AsStr();

            // str functions
            public RustStr m_AsRef() => m_AsStr().m_AsRef();
            public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DisplayFmt(crt_Core.mod_Fmt.s_Formatter p_0) => m_AsStr().DisplayFmt(p_0);
            public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DebugFmt(crt_Core.mod_Fmt.s_Formatter p_0) => m_AsStr().DebugFmt(p_0);
            public bool m_Eq(RustStr p_0) => m_AsStr().m_Eq(p_0);
            public bool m_Eq(s_String p_0) => m_AsStr().m_Eq(p_0.m_AsStr());
            public override bool Equals(object? obj) => obj is RustStr other && this.m_Eq(other);
            public static bool operator==(s_String lhs, RustStr rhs) => lhs.m_Eq(rhs);
            public static bool operator!=(s_String lhs, RustStr rhs) => !lhs.m_Eq(rhs);
            public static bool operator==(s_String lhs, s_String rhs) => lhs.m_Eq(rhs);
            public static bool operator!=(s_String lhs, s_String rhs) => !lhs.m_Eq(rhs);
            public crt_Core.mod_Option.s_Option<crt_Core.mod_Cmp.s_Ordering> m_PartialCmp(RustStr other) => m_AsStr().m_PartialCmp(other);
            public crt_Core.mod_Option.s_Option<crt_Core.mod_Cmp.s_Ordering> m_PartialCmp(s_String other) => m_AsStr().m_PartialCmp(other.m_AsStr());
            public crt_Core.mod_Cmp.s_Ordering m_Cmp(RustStr p_0) => m_AsStr().m_Cmp(p_0);
            public crt_Core.mod_Cmp.s_Ordering m_Cmp(s_String p_0) => m_AsStr().m_Cmp(p_0.m_AsStr());
            public RustStr m_Deref() => m_AsStr();
        }
    }
}

public static partial class RustStrExtensions
{
    extension(RustStr str)
    {
        public crt_Alloc.mod_String.s_String m_Replace(Rune from, RustStr to) => 
            str.Replace(from.ToBytes(stackalloc byte[4]), to.Inner);

        public crt_Alloc.mod_String.s_String m_ToOwned() => new(new crt_Alloc.mod_Vec.s_Vec<byte>(str.Inner.ToArray()));

        public static RustStr m_From(crt_Alloc.mod_String.s_String s) => s.m_AsStr();
        public static RustStr m_From(RustStr s) => s;
        public static RustStr m_From(string s) => ((crt_Alloc.mod_String.s_String)s).m_AsStr(); // TODO: remove

        private crt_Alloc.mod_String.s_String Replace(ReadOnlySpan<byte> from, ReadOnlySpan<byte> to)
        {
            var data = str.Inner;

            if (from.Length == 0)
            {
                // Special case: "": insert 'to' to every character boundary
                var result1 = crt_Alloc.mod_Vec.s_Vec<byte>.m_WithCapacity((nuint)(data.Length * (1 + to.Length)));
                for (var index = 0; index < data.Length; )
                {
                    result1.ExtendFromSpan(to);
                    var len = Utf8Utility.Utf8Len(data[index]);
                    result1.ExtendFromSpan(data.Slice(index, len));
                    index += len;
                }

                result1.ExtendFromSpan(to);

                return new crt_Alloc.mod_String.s_String(result1);
            }
            else
            {
                var result1 = crt_Alloc.mod_Vec.s_Vec<byte>.m_WithCapacity((nuint)(data.Length * (1 + to.Length)));

                for (;;)
                {
                    var index = data.IndexOf(from);
                    if (index < 0) break;
                    result1.m_Reserve((nuint)(index + to.Length));

                    result1.ExtendFromSpan(data.Slice(0, index));
                    data = data.Slice(index + from.Length);

                    result1.ExtendFromSpan(to);
                }

                result1.ExtendFromSpan(data);

                return new crt_Alloc.mod_String.s_String(result1);
            }
        }
    }

    // Box<RustStr>
    extension(crt_Core.mod_Option.s_Option<RustStr> self)
    {
        public crt_Core.mod_Option.s_Option<RustStr> m_AsDeref() => self;
    }

    extension(crt_Core.mod_Option.s_Option<crt_Alloc.mod_String.s_String> self)
    {
        public crt_Core.mod_Option.s_Option<RustStr> m_AsDeref() => self.m_Map(x => x.m_AsStr());
    }

    extension(string self)
    {
        // TODO: remove this
        public crt_Alloc.mod_String.s_String m_AsRef() => (crt_Alloc.mod_String.s_String)(self);
    }
}
