namespace System.Runtime.CompilerServices
{
    sealed class AsyncMethodBuilderAttribute : Attribute
    {
        public AsyncMethodBuilderAttribute(Type builderType)
        {
            BuilderType = builderType;
        }

        public Type BuilderType { get; }
    }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, AllowMultiple = false)]
    public class UnionAttribute : Attribute {}
    public class RequiredMemberAttribute : Attribute {}

    public class CompilerFeatureRequiredAttribute : Attribute
    {
        public CompilerFeatureRequiredAttribute(string featureName)
        {
            
        }
    }
}

namespace System
{
    public struct Index
    {
        private readonly int _value;
        public bool IsFromEnd => _value < 0;
        public int Value => _value < 0 ? ~_value : _value;

        public Index(int value, bool fromEnd = false)
        {
            if (value < 0)
            {
                ThrowOutOfRange();
                static void ThrowOutOfRange() => throw new ArgumentOutOfRangeException(nameof(value));
            }

            _value = fromEnd ? ~value : value;
        }

        public int ToIndex(int length) => IsFromEnd ? length - Value : Value; 
    }
}
