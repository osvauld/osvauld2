"""An educational apparatus assembles native wheel components without a vehicle API."""
from pathlib import Path
import tempfile
import base64
from osvauld.png import Image
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent

with tempfile.TemporaryDirectory(prefix='wheel-bench-') as directory:
    with Session(shell_binary=shell_binary(), data_dir=str(Path(directory)/'store'), offscreen=(1000,760)) as session:
        rpc=session.rpc
        rpc.signup('mechanics','isolated mechanics passphrase')
        workspace=rpc.create_workspace('Mechanics')
        item=rpc.create_item(workspace['id'],'Wheel bench','app')['id']
        rpc.upload_folder(item,ROOT/'demo_apps/mechanics_lab')
        rpc.open_item(item)
        def read():
            tree=rpc.dump_tree(item)
            assert 'worlds3d' in tree,(tree,rpc.read_console(item))
            return tree['worlds3d']['wheel-bench']
        def cart(state):
            return next(e for e in state['entities'] if e['authored']['id']=='carriage')
        ready=read()
        assert 'vehicles' not in ready
        assert len(ready['wheels'])==4
        assert abs(cart(ready)['mass']-20)<.01
        rpc.frame(30)
        assert read()==ready
        rpc.click_at(*rpc.centre_of('bench-run'))
        rpc.frame(180)
        settled=read()
        # Noninteractive scene leaves have no hit rect. Change only a visual color and
        # compare real shell pixels, preserving every native body/wheel throughout reload.
        before_pixels=Image(base64.b64decode(rpc.screenshot(item)['png_base64']))
        source=rpc.read_file_versioned(item,'main.lua')
        old='{id="carriage",scale=C.size,color=C.accent}'
        new='{id="carriage",scale=C.size,color="#ff3333"}'
        result=rpc.edit_file(item,'main.lua',source['revision'],[{'old_text':old,'new_text':new}])
        assert result['activation']=='activated',result
        after_pixels=Image(base64.b64decode(rpc.screenshot(item)['png_base64']))
        assert (before_pixels.width,before_pixels.height)==(after_pixels.width,after_pixels.height)
        changed=sum(a!=b for ra,rb in zip(before_pixels.rows,after_pixels.rows) for a,b in zip(ra,rb))
        assert changed>1500, 'carriage color edit did not reach rendered pixels'
        assert read()==settled, 'visual edit/capture changed native physics'
        source=rpc.read_file_versioned(item,'main.lua')
        result=rpc.edit_file(item,'main.lua',source['revision'],[{'old_text':new,'new_text':old}])
        assert result['activation']=='activated',result
        rpc.click(item,'bench-run')
        assert all(w['contact'] for w in settled['wheels'])
        rpc.click_at(*rpc.centre_of('bench-motor'))
        rpc.frame(120)
        moving=read()
        assert cart(moving)['resolved']['position'][2]>cart(settled)['resolved']['position'][2]+.5
        assert all(w['input']['motor'] and abs(w['input']['motor']['max_torque']-1)<1e-6 for w in moving['wheels'])
        rpc.click(item,'bench-coast')
        assert all(w['input']['motor'] is None and w['input']['torque']==0 for w in read()['wheels'])
        rpc.frame(30)
        rpc.click(item,'bench-brake')
        rpc.frame(180)
        assert abs(cart(read())['resolved']['velocity'][2])<.15
        rpc.click(item,'bench-pause')
        paused=read()
        rpc.frame(120)
        rpc.screenshot(item)
        rpc.screenshot(item,width=1200,height=800,scale=1)
        assert read()==paused
        rpc.save_screenshot(item,ROOT/'shots/wheel-bench.png')
        assert not rpc.read_console(item),rpc.read_console(item)
        def nodes(t):
            yield t
            for child in t.get('children',[]): yield from nodes(child)
        scene=next(n['scene3d'] for n in nodes(rpc.dump_tree(item)) if n.get('id')=='bench-view')
        for wheel in paused['wheels']:
            drawn=next(o for o in scene['objects'] if o['id']==wheel['id'])
            assert drawn['position']==wheel['position']
            assert max(abs(a-b) for a,b in zip(drawn['rotation'],wheel['rotation']))<1e-6
    with Session(shell_binary=shell_binary(),data_dir=str(Path(directory)/'store'),offscreen=(1000,760)) as session:
        rpc=session.rpc
        rpc.unlock('mechanics','isolated mechanics passphrase')
        rpc.open_item(item)
        world=rpc.dump_tree(item)['worlds3d']['wheel-bench']
        assert len(world['wheels'])==4 and 'vehicles' not in world
        assert all(w['input']['motor'] is None for w in world['wheels'])
        assert not rpc.read_console(item),rpc.read_console(item)
        print('physics components ok: independent educational apparatus, motor, freewheel, brake and paused captures')
