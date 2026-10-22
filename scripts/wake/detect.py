"""点数（80ms ごと、0〜1）から「呼ばれた」を決める。numpy は要らない（`test_wake.py` が py -3 で検査する）。

- しきい値を超えたフレームが `consecutive` 回続いたら 1 回反応する（1 フレームだけの跳ねで反応しない）
- 反応した後 `refractory` フレームは次を出さない（語の終わりの後も数フレームは点数が高いまま残るので、1 回の呼びかけで 2 回反応しない）
"""

FRAME_SEC = 0.08  # 1 フレーム＝1280 サンプル（16kHz）


class Gate:
    def __init__(self, threshold, consecutive=2, refractory=25):
        if not 0.0 < threshold < 1.0:
            raise ValueError("threshold は 0 と 1 の間")
        if consecutive < 1 or refractory < 0:
            raise ValueError("consecutive は 1 以上、refractory は 0 以上")
        self.threshold = threshold
        self.consecutive = consecutive
        self.refractory = refractory
        self._run = 0
        self._cool = 0

    def push(self, score):
        """1 フレーム分の点数を入れ、このフレームで反応したら True"""
        if self._cool > 0:
            self._cool -= 1
            self._run = 0
            return False
        self._run = self._run + 1 if score >= self.threshold else 0
        if self._run >= self.consecutive:
            self._run = 0
            self._cool = self.refractory
            return True
        return False


def count_events(scores, threshold, consecutive=2, refractory=25):
    """点数の並びで何回反応するか（判定の道具が誤反応を数えるのに使う）"""
    g = Gate(threshold, consecutive, refractory)
    return sum(1 for s in scores if g.push(s))
