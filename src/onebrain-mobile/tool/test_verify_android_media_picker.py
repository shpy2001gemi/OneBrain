import unittest
from unittest.mock import patch

from tool.verify_android_media_picker import (
    PICKER_FILE,
    PickerControls,
    picker_controls,
    wait_and_select_picker,
)


class AndroidMediaPickerHarnessTests(unittest.TestCase):
    def test_uses_lower_left_file_target_and_finds_confirmation(self) -> None:
        xml = f"""<?xml version='1.0' encoding='UTF-8'?>
<hierarchy>
  <node text="" content-desc="{PICKER_FILE}, 68 B" resource-id=""
        clickable="false" enabled="true" bounds="[100,200][500,600]" />
  <node text="" content-desc="Preview the file {PICKER_FILE}"
        resource-id="com.google.android.documentsui:id/preview_icon"
        clickable="true" enabled="true" bounds="[350,200][500,350]" />
  <node text="Open" content-desc="" resource-id=""
        clickable="true" enabled="true" bounds="[700,800][900,900]" />
</hierarchy>"""
        with patch(
            "tool.verify_android_media_picker.adb",
            side_effect=["", xml],
        ):
            controls = picker_controls("adb", "emulator")

        self.assertEqual(controls.file_location, (200, 500))
        self.assertEqual(controls.confirmation_location, (800, 850))

    def test_ignores_preview_affordance_as_confirmation(self) -> None:
        xml = f"""<?xml version='1.0' encoding='UTF-8'?>
<hierarchy>
  <node text="" content-desc="{PICKER_FILE}, 68 B" resource-id=""
        clickable="false" enabled="true" bounds="[0,0][200,200]" />
  <node text="" content-desc="Preview the file {PICKER_FILE}"
        resource-id="com.google.android.documentsui:id/preview_icon"
        clickable="true" enabled="true" bounds="[100,0][200,100]" />
</hierarchy>"""
        with patch(
            "tool.verify_android_media_picker.adb",
            side_effect=["", xml],
        ):
            controls = picker_controls("adb", "emulator")

        self.assertEqual(controls.file_location, (50, 150))
        self.assertIsNone(controls.confirmation_location)

    def test_selects_once_then_confirms_before_waiting_for_resume(self) -> None:
        with (
            patch(
                "tool.verify_android_media_picker.picker_controls",
                side_effect=[
                    PickerControls((100, 200), None),
                    PickerControls((100, 200), (300, 400)),
                ],
            ),
            patch(
                "tool.verify_android_media_picker.app_is_resumed",
                side_effect=[False, True],
            ),
            patch(
                "tool.verify_android_media_picker.time.monotonic",
                side_effect=[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            ),
            patch("tool.verify_android_media_picker.time.sleep"),
            patch("tool.verify_android_media_picker.adb", return_value="") as adb_mock,
        ):
            wait_and_select_picker("adb", "emulator", 30)

        taps = [
            call.args[-2:]
            for call in adb_mock.call_args_list
            if call.args[2:5] == ("shell", "input", "tap")
        ]
        self.assertEqual(taps, [("100", "200"), ("300", "400")])


if __name__ == "__main__":
    unittest.main()
