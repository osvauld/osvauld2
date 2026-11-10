"""Real rover diagnostics: native wall-clock work, unavailable offscreen FPS and overlay pixels."""
import base64
import math
import os
import time
from pathlib import Path

from osvauld.png import Image
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
os.environ['OSVAULD_PERFORMANCE'] = '1'
with Session(shell_binary=shell_binary(), offscreen=(1000, 760), show_shell_output=True) as session:
    rpc = session.rpc
    assert rpc.request('Performance')['enabled']
    rpc.signup('performance', 'isolated performance passphrase')
    workspace = rpc.create_workspace('Performance')
    item = rpc.create_item(workspace['id'], 'Moon rover', 'app')['id']
    rpc.upload_folder(item, ROOT / 'demo_apps/moon_rover')
    rpc.open_item(item)
    rpc.click(item, 'rover-drive')
    rpc.frame(180)
    rpc.keyboard('KeyW', 'w', True)
    rpc.frame(60)
    rpc.keyboard('KeyW', 'w', False)
    rpc.click(item, 'course-pause')
    # Diagnostics publish on wall time; this wait spends no simulation time.
    time.sleep(0.3)
    rpc.frame(1)
    world = rpc.dump_tree(item)['worlds3d']['lunar-course']
    stats = rpc.request('Performance')
    assert stats['mode'] == 'offscreen' and stats['fps'] is None, stats
    assert stats['presented_frames'] == 0, stats
    assert 0 < stats['samples'] <= stats['sample_capacity'] == 240, stats
    assert stats['normal_frames'] >= 240, stats
    for value in stats['work_ms'].values():
        assert math.isfinite(value) and value >= 0, stats
    assert stats['work_ms']['max'] >= stats['work_ms']['p95'] >= stats['work_ms']['p50'], stats
    assert stats['gpu_ms'] is None and stats['adapter'], stats
    assert stats['target_fps'] == 60 and abs(stats['budget_ms'] - 1000 / 60) < 0.001, stats
    assert max(abs(a-b) for a,b in zip(world['gravity'],[0,-1.62,0])) < 1e-6, world['gravity']
    assert len(world['entities']) == 4 and len(world['wheels']) == 4
    assert 'vehicles' not in world
    assert world['dropped_seconds'] == 0, world
    terrain = next(e['authored']['shape']['mesh'] for e in world['entities'] if e['authored']['id'] == 'course')
    assert terrain['triangles'] == 6144

    image = Image(base64.b64decode(rpc.screenshot(item)['png_base64']))
    x, y, width, height = stats['overlay_rect']
    # The opaque diagnostic plate and bright glyphs must reach actual shell pixels.
    colors = [image.at(x + dx, y + dy) for dx in range(8, int(width) - 8) for dy in range(8, int(height) - 8)]
    assert sum(all(c > 130 for c in color[:3]) for color in colors) > 100, 'missing performance glyphs'
    assert sum(all(c < 45 for c in color[:3]) for color in colors) > 1000, 'missing performance plate'
    rpc.save_screenshot(item, ROOT / 'shots/rover-performance.png')
    rpc.screenshot(item, width=1200, height=800, scale=1)
    assert rpc.dump_tree(item)['worlds3d']['lunar-course'] == world, 'diagnostics/captures advanced paused physics'
    assert not rpc.read_console(item), rpc.read_console(item)
    print('rover performance ok: real work samples, offscreen FPS unavailable, bounded history, overlay pixels and physics audit')

os.environ['OSVAULD_PERFORMANCE'] = '0'
with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as session:
    assert session.rpc.performance() == {'enabled': False}
    session.rpc.frame(60)
    assert session.rpc.performance() == {'enabled': False}
    print('disabled performance ok: no native diagnostic snapshots')
