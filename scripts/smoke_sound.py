"""Sound on the real path (plan: docs/design/sound.md §0, S-1..S-2): a tiny app plays a generated
beep on click, and the shell's mixer — on the virtual clock offscreen — logs it and hands back the
real samples. Bad synth specs are console errors naming the field."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary

RATE = 48_000
APP = """
local beep = sound.synth{ name = "beep", wave = "square", freq = 660, sustain = 0.1, decay = 0.05, volume = 0.5 }
local function button(id, on_click)
	return ui.button({ id = id, px = 12, py = 8, on_click = on_click, ui.text({ id, no_wrap = true }) })
end
return function()
	return ui.col({
		id = "root", gap = 8,
		button("beep", function() sound.play(beep) end),
		button("high", function() sound.play(beep, { pitch = 2, volume = 0.5 }) end),
		button("bogus", function() sound.synth{ name = "x", bogus = 1 } end),
		button("tuba", function() sound.synth{ wave = "tuba" } end),
		button("loud", function() sound.play(beep, { volume = 3 }) end),
	})
end
"""


def rate_of(samples):
    ups = sum(1 for a, b in zip(samples, samples[1:]) if a < 0 <= b)
    return ups * RATE / len(samples)


def beep_after_click(rpc, item, button, starts_before):
    rpc.click(item, button)
    rpc.frame(30)
    log = rpc.sound_log(item)
    starts = [e for e in log if e["event"] == "start"]
    assert len(starts) == starts_before + 1, log
    start = starts[-1]
    assert start["name"] == "beep", start
    stop = [e for e in log if e["event"] == "stop" and e["voice"] == start["voice"]]
    assert stop and stop[0]["why"] == "ended", log
    assert abs(stop[0]["at"] - start["at"] - 0.15) < 1e-3, (start, stop)

    samples, clock = rpc.sound_capture(item, 0.75)
    at = round((start["at"] - (clock - 0.75)) * RATE)
    assert 0 < at < len(samples) - 7200, (at, clock, start)
    assert all(v == 0 for v in samples[:at]), "silent before the click"
    played = samples[at:at + round(0.1 * RATE)]
    assert max(abs(v) for v in played[:800]) > 0.2, "plays at once, not a frame late"
    return start, rate_of(played)


def main():
    with Session(shell_binary=shell_binary(), offscreen=(600, 400)) as s:
        rpc = s.rpc
        rpc.signup("listener", "listener passphrase")
        ws = rpc.create_workspace("sound")
        item = rpc.create_item(ws["id"], "Beeper", "app")["id"]
        rpc.write_file(item, "main.lua", APP)
        rpc.open_item(item)
        rpc.frame(30)
        assert not rpc.read_console(item), rpc.read_console(item)

        # S-1: nothing plays until asked; a click plays the beep, logged and heard, at its pitch.
        assert rpc.sound_log(item) == []
        quiet, _ = rpc.sound_capture(item, 0.5)
        assert len(quiet) == RATE // 2 and not any(quiet), "silent at rest"
        start, freq = beep_after_click(rpc, item, "beep", 0)
        assert abs(freq - 660) < 15, freq
        assert start["pitch"] == 1 and start["volume"] == 1, start
        start, freq = beep_after_click(rpc, item, "high", 1)
        assert abs(freq - 1320) < 25, freq
        assert start["pitch"] == 2 and start["volume"] == 0.5, start
        print(f"S-1 ok: beep logged and heard at {freq:.0f} Hz with pitch 2")

        # S-2: strict specs and options, named on the console.
        for button, expect in [
            ("bogus", "sound.synth: unknown field bogus"),
            ("tuba", "wave must be square, saw, sine or noise, got tuba"),
            ("loud", "sound.play: volume must be 0 to 1, got 3"),
        ]:
            rpc.click(item, button)
            rpc.frame(2)
            console = "\n".join(rpc.read_console(item))
            assert expect in console, (button, console)
        assert len([e for e in rpc.sound_log(item) if e["event"] == "start"]) == 2, "errors play nothing"
        print("S-2 ok: bad specs are console errors naming the field")


if __name__ == "__main__":
    main()
    print("sound smoke: ok")
