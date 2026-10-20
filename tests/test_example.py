import pytest

from src.example import add, divide, multiply, subtract


class TestAdd:
    def test_正の数を足すと正しい結果が返る(self) -> None:
        assert add(2, 3) == 5

    def test_負の数を足すと正しい結果が返る(self) -> None:
        assert add(-2, 3) == 1


class TestSubtract:
    def test_正の数を引くと正しい結果が返る(self) -> None:
        assert subtract(5, 3) == 2


class TestMultiply:
    def test_正の数をかけると正しい結果が返る(self) -> None:
        assert multiply(2, 3) == 6

    def test_0をかけると0が返る(self) -> None:
        assert multiply(5, 0) == 0


class TestDivide:
    def test_正の数で割ると正しい結果が返る(self) -> None:
        assert divide(6, 3) == 2

    def test_0で割るとValueErrorが発生する(self) -> None:
        with pytest.raises(ValueError, match="Division by zero is not allowed"):
            divide(5, 0)
