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
        public Index(int value, bool fromEnd = false) => throw null;
    }
}
