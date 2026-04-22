/// UI-style tests for match_patterns.rs
/// Mirrors patterns from rustc tests/ui/match/
public class UI_MatchPatternsTests
{
    [Fact] public void DirectionNorth() =>
        Assert.Equal(0, mod_match_patterns.m_direction_value(
            new mod_match_patterns.s_Direction { f_discriminant = mod_match_patterns.s_Direction.k_North }));

    [Fact] public void DirectionSouth() =>
        Assert.Equal(1, mod_match_patterns.m_direction_value(
            new mod_match_patterns.s_Direction { f_discriminant = mod_match_patterns.s_Direction.k_South }));

    [Fact] public void DirectionEast() =>
        Assert.Equal(2, mod_match_patterns.m_direction_value(
            new mod_match_patterns.s_Direction { f_discriminant = mod_match_patterns.s_Direction.k_East }));

    [Fact] public void DirectionWest() =>
        Assert.Equal(3, mod_match_patterns.m_direction_value(
            new mod_match_patterns.s_Direction { f_discriminant = mod_match_patterns.s_Direction.k_West }));

    [Fact] public void CategorizeNegative() => Assert.Equal(-1, mod_match_patterns.m_categorize(-10));
    [Fact] public void CategorizeZero()     => Assert.Equal(0,  mod_match_patterns.m_categorize(0));
    [Fact] public void CategorizeOne()      => Assert.Equal(1,  mod_match_patterns.m_categorize(5));
    [Fact] public void CategorizeTwoDigit() => Assert.Equal(2,  mod_match_patterns.m_categorize(42));
    [Fact] public void CategorizeHuge()     => Assert.Equal(3,  mod_match_patterns.m_categorize(1000));

    [Fact] public void FizzBuzz3()  => Assert.Equal(3,  mod_match_patterns.m_fizzbuzz(3));
    [Fact] public void FizzBuzz5()  => Assert.Equal(5,  mod_match_patterns.m_fizzbuzz(5));
    [Fact] public void FizzBuzz15() => Assert.Equal(15, mod_match_patterns.m_fizzbuzz(15));
    [Fact] public void FizzBuzz7()  => Assert.Equal(7,  mod_match_patterns.m_fizzbuzz(7));
}
