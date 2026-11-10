"""Rover driving through real keys/pointer, native suspension, rendered wheels and a chase camera."""
from pathlib import Path
import math

from osvauld.scene3d import assert_3d_pixels
from osvauld.session import Session,shell_binary

ROOT=Path(__file__).resolve().parent.parent

def nodes(tree):
    yield tree
    for c in tree.get('children',[]):
        yield from nodes(c)

with Session(shell_binary=shell_binary(),offscreen=(1000,760)) as s:
    r=s.rpc
    r.signup('driver','rover smoke passphrase')
    ws=r.create_workspace('Rover drive')
    item=r.create_item(ws['id'],'Moon rover','app')['id']
    r.upload_folder(item,ROOT/'demo_apps/moon_rover')
    r.open_item(item)

    def read():
        tree=r.dump_tree(item)
        assert 'worlds3d' in tree,(tree,r.read_console(item))
        return tree['worlds3d']['lunar-course']

    def chassis(state):
        return next(e['resolved'] for e in state['entities'] if e['authored']['id']=='rover')

    def wheels(state):
        return state['wheels']

    def scene():
        return next(n['scene3d'] for n in nodes(r.dump_tree(item)) if n.get('id')=='course-view')

    r.click_at(*r.centre_of('rover-drive'))
    r.frame(180)
    settled=read()
    assert all(w['contact'] for w in wheels(settled)),settled
    assert abs(next(e['mass'] for e in settled['entities'] if e['authored']['id']=='rover')-400)<0.01
    assert_3d_pixels(r,item,'course-view','rover',logical_width=1000)
    r.save_screenshot(item,ROOT/'shots/rover-ready.png')

    r.keyboard('KeyW','w',True)
    r.frame(120)
    r.keyboard('KeyW','w',False)
    forward=read()
    assert chassis(forward)['position'][2]>chassis(settled)['position'][2]+1,forward
    view=scene()
    for wheel in wheels(forward):
        visual=next(o for o in view['objects'] if o['id']==wheel['id'])
        assert visual['position']==wheel['position'],(visual,wheel)
        assert max(abs(a-b) for a,b in zip(visual['rotation'],wheel['rotation']))<1e-6,(visual,wheel)
        assert 0.09<=wheel['suspension_length']<=0.47,wheel
    at=chassis(forward)['position']
    assert sum((a-b)**2 for a,b in zip(view['camera']['target'],at))<2,view
    assert 0.8<view['camera']['eye'][1]<4,view
    r.keyboard('KeyD','d',True)
    r.frame(45)
    r.keyboard('KeyD','d',False)
    turned=read()
    assert abs(chassis(turned)['rotation'][1])>0.03,turned
    q=chassis(turned)['rotation']
    assert 2*(q[0]*q[2]+q[1]*q[3]) < -0.05,('D steered left',turned)
    r.keyboard('KeyW','w',True)
    r.keyboard('Space',' ',True)
    assert all(w['input']['motor'] is None and w['input']['brake']==105 for w in wheels(read())), 'Lua brake priority failed'
    r.frame(180)
    stopped=read()
    velocity=chassis(stopped)['velocity']
    assert math.hypot(velocity[0],velocity[2])<0.2,stopped
    r.keyboard('Space',' ',False)
    assert all(w['input']['motor'] and w['input']['motor']['max_torque']==42 for w in wheels(read()))
    r.keyboard('KeyW','w',False)
    r.save_screenshot(item,ROOT/'shots/rover-driven.png')

    # Held mouse pedals use actual hit geometry; leaving releases them even while held.
    gas=r.centre_of('pedal-gas')
    r.move_to(*gas)
    r.press()
    r.frame(30)
    assert all(w['input']['motor'] and w['input']['motor']['max_torque']==42 for w in wheels(read()))
    r.move_to(*r.centre_of('pedal-brake'))
    r.frame(1)
    controls=[w['input'] for w in wheels(read())]
    assert all(c['motor'] is None and c['brake']==105 for c in controls),controls
    r.release()
    r.frame(1)
    assert all(w['input']['brake']==0 for w in wheels(read()))

    r.keyboard('KeyP','p',True)
    r.keyboard('KeyP','p',False)
    paused=read()
    r.frame(60)
    r.screenshot(item)
    r.screenshot(item,width=1200,height=800,scale=1)
    assert read()==paused,'paused inspection/capture advanced a vehicle'
    before_camera=scene()['camera']
    r.keyboard('ArrowRight','ArrowRight',True)
    r.frame(20)
    r.keyboard('ArrowRight','ArrowRight',False)
    after_camera=scene()['camera']
    eye,target=before_camera['eye'],before_camera['target']
    right=[eye[2]-target[2],0,target[0]-eye[0]]
    assert sum((b-a)*c for a,b,c in zip(eye,after_camera['eye'],right))>0,('right arrow orbited left',before_camera,after_camera)
    assert read()==paused,'camera arrows advanced paused physics'
    r.keyboard('KeyR','r',True)
    r.keyboard('KeyR','r',False)
    reset=read()
    assert max(abs(a-b) for a,b in zip(chassis(reset)['position'],[-10,1.2,-8]))<0.01,reset
    assert all(w['input']['motor'] is None and w['input']['steering']==0 for w in wheels(reset))
    # The real mint block is below the chassis floor; its side must meet a solid wheel.
    r.click(item,'course-reset')
    r.click(item,'rover-drive')
    r.frame(180)
    def block_position(state):
        return next(e['resolved']['position'] for e in state['entities'] if e['authored']['id']=='box')
    before_block=block_position(read())
    r.keyboard('KeyW','w',True)
    distance=0
    for _ in range(6):
        r.frame(60)
        after_block=block_position(read())
        distance=math.hypot(after_block[0]-before_block[0],after_block[2]-before_block[2])
        if distance>0.5: break
    r.keyboard('KeyW','w',False)
    assert distance>0.5,('wheel passed through mint block',before_block,after_block)
    r.click(item,'course-pause')
    r.keyboard('ArrowLeft','ArrowLeft',True)
    r.frame(60)
    r.keyboard('ArrowLeft','ArrowLeft',False)
    r.save_screenshot(item,ROOT/'shots/rover-block-contact.png')

    # Change only the reset orientation through a revision-checked edit, preserving the solver.
    source=r.read_file_versioned(item,'vehicle.lua')
    result=r.edit_file(item,'vehicle.lua',source['revision'],[{
        'old_text':'return {id = "rover", position = V.spawn, box = V.size, mass = V.mass,',
        'new_text':'return {id = "rover", rotation={0,math.sin(math.pi/4),0,math.cos(math.pi/4)}, position = V.spawn, box = V.size, mass = V.mass,',
    }])
    assert result['activation']=='activated',result
    r.click(item,'course-reset')
    r.click(item,'rover-drive')
    r.frame(180)
    r.keyboard('KeyW','w',True)
    visited=[]
    for _ in range(18):
        r.frame(30)
        current=read()
        pose=chassis(current)
        visited.append(pose['position'][0])
        assert pose['position'][1]>-1.1,pose
        for wheel in wheels(current):
            assert 0.09<=wheel['suspension_length']<=0.47,wheel
        if pose['position'][0]>7: break
    r.keyboard('KeyW','w',False)
    assert max(visited)>7,visited
    r.save_screenshot(item,ROOT/'shots/rover-terrain-drive.png')
    r.click(item,'course-reset')
    assert not r.read_console(item),r.read_console(item)
    print('rover drive ok: contacts, low-block collision, mass, keys, steering, braking, wheel pixels/poses, chase camera, held pedals, pause/capture and reset')
