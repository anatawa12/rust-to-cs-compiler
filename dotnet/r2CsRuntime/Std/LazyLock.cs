namespace VrcGetVpm;

public static partial class crt_Std
{
    public static partial class mod_Sync
    {
        public static partial class mod_LazyLock
        {
            public class s_LazyLock<T, Action>
            {
                private T _value;

                public s_LazyLock(Func<T> initializer)
                {
                    _value = initializer(); // Note: Semantics difference: called on initialize. This is intended difference
                }

                public static s_LazyLock<T, Action> m_New(Func<T> func) => new(func);

                public T m_Deref() => _value;
            }
        }
    }
}