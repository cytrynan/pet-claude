"""スキルの道具のテスト（純粋な部分だけ。Comfy と ffmpeg は呼ばない）。

    python -I -m unittest discover -s .claude/skills/make-pet-clips/scripts -p "test_make_pet_clips.py"
"""

import json
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import align_poses  # noqa: E402
import make_runs  # noqa: E402

TEMPLATE = json.loads(make_runs.TEMPLATE.read_text(encoding="utf-8"))
FIELDS = {"subject": "cute chibi baby panda", "noun": "panda", "design": "thick outline, white fur",
          "idle": " with its paws together", "wiggle": "its round ears twitch slightly", "hand": "paw"}


class MakeRunsTest(unittest.TestCase):
    def test_renders_13_clips_in_3_runs_with_no_placeholder_left(self):
        runs = make_runs.render(TEMPLATE, "pet", 960, FIELDS, "green, dragon")
        self.assertEqual([len(r["clips"]) for r in (runs["run1"], runs["run2"], runs["run3"])], [4, 8, 3])
        for run in runs.values():
            self.assertEqual(run["size"], [960, 960])
            self.assertIn("green, dragon", run["negative_extra"])
            for c in run["clips"]:
                self.assertNotIn("{", c["prompt"])
                self.assertEqual((c["length"] - 1) % 4, 0)
                self.assertTrue(c["name"].startswith("pet_"))
                self.assertIn("cute chibi baby panda", c["prompt"])
        # なでられだけ手のネガティブが付き、指示文に「なでる・触る」が無い（2026-10-21 の教訓）
        self.assertIn("petting", runs["run3"]["negative_extra"])
        self.assertNotIn("petting", runs["run1"]["negative_extra"])
        for c in runs["run3"]["clips"]:
            self.assertNotIn("pet", c["prompt"].replace("pet_", ""))
            self.assertNotIn("touch", c["prompt"])

    def test_missing_field_is_an_error(self):
        with self.assertRaises(KeyError):
            make_runs.render(TEMPLATE, "pet", 640, {k: v for k, v in FIELDS.items() if k != "wiggle"}, "")

    def test_clips_src_matches_the_runs(self):
        runs = make_runs.render(TEMPLATE, "pet", 960, FIELDS, "")
        made = {c["name"] for r in runs.values() for c in r["clips"]}
        src = make_runs.clips_src("pet", 960, "poses")
        self.assertEqual(len(src["clips"]), 13)
        self.assertEqual(src["canvas"], 960)
        for c in src["clips"]:
            video = Path(c["video"]).stem
            self.assertIn(video, made, f"{c['name']} の動画 {video} が run に無い")
        self.assertEqual([c for c in src["clips"] if c["name"] == "walk_start"][0]["drop_tail"], 0)


class AlignPosesTest(unittest.TestCase):
    def test_find_states_by_name_and_floor_overrides(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            for n in ["01-idle.png", "02-walk.png", "07-sleeping.png", "notes.png"]:
                (Path(d) / n).write_bytes(b"")
            found = align_poses.find_states(Path(d))
        self.assertEqual(sorted(found), ["idle", "sleeping", "walk"])
        floors = align_poses.parse_floors(["happy=560", "dragged=540"])
        self.assertEqual((floors["happy"], floors["dragged"]), (560, 540))
        self.assertEqual(align_poses.parse_floors(None), {"dragged": 560})


if __name__ == "__main__":
    unittest.main()
