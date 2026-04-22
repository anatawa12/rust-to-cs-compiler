/// UI-style tests for enum_patterns.rs
/// Mirrors patterns from rustc tests/ui/match/ and tests/ui/if-let/
public class UI_EnumPatternsTests
{
    static mod_enum_patterns.s_Tree Leaf(int v) =>
        new mod_enum_patterns.s_Tree
        {
            f_discriminant = mod_enum_patterns.s_Tree.k_Leaf,
            f_Leaf = new mod_enum_patterns.s_Tree.s_Tree_Leaf { f_0 = v }
        };

    static mod_enum_patterns.s_Tree Node(int l, int r) =>
        new mod_enum_patterns.s_Tree
        {
            f_discriminant = mod_enum_patterns.s_Tree.k_Node,
            f_Node = new mod_enum_patterns.s_Tree.s_Tree_Node { f_0 = l, f_1 = r }
        };

    static mod_enum_patterns.s_MaybeInt Some(int v) =>
        new mod_enum_patterns.s_MaybeInt
        {
            f_discriminant = mod_enum_patterns.s_MaybeInt.k_Some,
            f_Some = new mod_enum_patterns.s_MaybeInt.s_MaybeInt_Some { f_0 = v }
        };

    static mod_enum_patterns.s_MaybeInt None() =>
        new mod_enum_patterns.s_MaybeInt { f_discriminant = mod_enum_patterns.s_MaybeInt.k_None };

    [Fact] public void TreeSumLeaf() => Assert.Equal(7,  mod_enum_patterns.m_tree_sum(Leaf(7)));
    [Fact] public void TreeSumNode() => Assert.Equal(11, mod_enum_patterns.m_tree_sum(Node(4, 7)));

    [Fact] public void MaxLeaf()     => Assert.Equal(5,  mod_enum_patterns.m_max_of_tree(Leaf(5)));
    [Fact] public void MaxNodeLeft() => Assert.Equal(9,  mod_enum_patterns.m_max_of_tree(Node(9, 3)));
    [Fact] public void MaxNodeRight() => Assert.Equal(8, mod_enum_patterns.m_max_of_tree(Node(3, 8)));

    [Fact] public void UnwrapSome()    => Assert.Equal(42, mod_enum_patterns.m_unwrap_or(Some(42), 0));
    [Fact] public void UnwrapNone()    => Assert.Equal(99, mod_enum_patterns.m_unwrap_or(None(), 99));

    [Fact] public void MapDoubleSome() => Assert.Equal(10, mod_enum_patterns.m_map_double(Some(5)).f_Some.f_0);
    [Fact] public void MapDoubleNone() => Assert.Equal(mod_enum_patterns.s_MaybeInt.k_None,
        mod_enum_patterns.m_map_double(None()).f_discriminant);

    [Fact] public void IsSomeTrue()  => Assert.True(mod_enum_patterns.m_is_some(Some(1)));
    [Fact] public void IsSomeFalse() => Assert.False(mod_enum_patterns.m_is_some(None()));
}
