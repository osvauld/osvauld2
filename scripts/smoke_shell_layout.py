"""Real-shell auth/workspace geometry at phone/desktop widths; pointer-opened popovers.

RPC drives account/workspace setup. Keyboard RPC is game input, not editor text injection;
real Android field typing is exercised separately with touch and ADB hardware-key injection.
"""
import time

from osvauld.session import Session, build_shell, shell_binary


def wait_for(check):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if check():
            return
        time.sleep(0.05)
    raise AssertionError("shell screen did not transition within 15 seconds")


def field(rpc, element, width, expected=None):
    rect = next(r for r in rpc.rects() if r["id"] == element)
    assert rect["w"] > 0 and rect["h"] > 0, rect
    assert 0 <= rect["x"] and rect["x"] + rect["w"] <= width, rect
    assert 0 <= rect["y"] and rect["y"] + rect["h"] <= 720, rect
    if expected is not None:
        assert abs(rect["w"] - expected) <= 0.5, rect
    return rect


build_shell()
for width in (320, 393, 900):
    with Session(shell_binary=shell_binary(), offscreen=(width, 720)) as session:
        rpc = session.rpc
        for element in ("label", "password", "submit"):
            field(rpc, element, width, min(width - 32, 360))
        rpc.signup("layout", "demo")  # RPC skips mnemonic display; never capture its secrets.
        rpc.lock()
        field(rpc, "passphrase", width, min(width - 32, 460))
        field(rpc, "login", width, min(width - 32, 460))
        rpc.unlock("layout", "demo")
        # Past auth the Lua launcher owns the list and creation form; claim stays Rust chrome.
        wait_for(lambda: any(r["id"] == "launcher:new-workspace" for r in rpc.rects()))
        field(rpc, "launcher:new-workspace", width)
        field(rpc, "claim_node", width)
        rpc.click_at(*rpc.centre_of("launcher:new-workspace"))
        for element in ("launcher:create:name", "launcher:create:submit", "launcher:create:cancel"):
            field(rpc, element, width)
        rpc.click_at(*rpc.centre_of("launcher:create:cancel"))
        rpc.create_workspace("Phone")
        wait_for(lambda: len(rpc.list_workspaces()) == 1)
        space = "launcher:space:" + rpc.list_workspaces()[0]["id"]
        wait_for(lambda: any(r["id"] == space for r in rpc.rects()))
        field(rpc, space, width)
        rpc.click_at(*rpc.centre_of("claim_node"))
        field(rpc, "claim_ticket", width, min(width * 0.9, 360) - 24)
        print(f"shell layout {width}x720: signup, unlock, launcher creation and claim popover ok")
