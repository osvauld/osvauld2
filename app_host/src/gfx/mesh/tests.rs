use mlua::{AnyUserData, Lua};

fn lua() -> Lua {
    let lua = Lua::new();
    crate::gfx::install(&lua).unwrap();
    lua.load(
        r#"
        V = {
            { position = {-1, -1, 0}, normal = {0, 0, 1} },
            { position = { 1, -1, 0}, normal = {0, 0, 1} },
            { position = { 0,  1, 0}, normal = {0, 0, 1} },
        }
        function make() return gfx.mesh({ vertices = V, indices = {1, 2, 3} }) end
    "#,
    )
    .exec()
    .unwrap();
    lua
}

#[test]
fn lua_mesh_handles_are_shared_and_inspectable() {
    let lua = lua();
    let handle: AnyUserData = lua
        .load(
            r#"
        local mesh = make()
        return gfx.scene3d({
            camera = { eye = {0, 0, 5}, target = {0, 0, 0} },
            objects = {
                { id = "a", mesh = mesh, color = '#ffffff' },
                { id = "b", mesh = mesh, color = '#ff0000', position = {2, 0, 0} },
            },
        })
    "#,
        )
        .eval()
        .unwrap();
    let scene = handle.borrow::<crate::gfx::LuaScene3d>().unwrap();
    let info = scene.0.inspect();
    assert_eq!(info.objects[0].mesh_resource, info.objects[1].mesh_resource);
    assert_eq!(info.objects[0].vertex_count, 3);
    assert_eq!(info.objects[0].triangle_count, 1);
    assert_eq!(
        info.objects[0].local_bounds,
        ([-1.0, -1.0, 0.0], [1.0, 1.0, 0.0])
    );
    let hit = scene.0.raycast((200.0, 200.0), (100.0, 100.0)).unwrap();
    assert_eq!(hit.id.as_ref(), "a");
}

#[test]
fn mesh_tables_indices_and_vertices_are_strict() {
    let lua = lua();
    for bad in [
        "{vertices=V, indices={1,2,3}, typo=true}",
        "{vertices={[1]=V[1], [3]=V[3]}, indices={1,2,3}}",
        "{vertices=V, indices={1,2,3, named=1}}",
        "{vertices=V, indices={1,2}}",
        "{vertices=V, indices={0,2,3}}",
        "{vertices=V, indices={1.5,2,3}}",
        "{vertices=V, indices={'1',2,3}}",
        "{vertices=V, indices={1,2,4}}",
        "{vertices=V, indices={1,1,3}}",
        "{vertices={{position={0,0,0}, normal={0,0,0}}, V[2], V[3]}, indices={1,2,3}}",
        "{vertices={{position={0/0,0,0}, normal={0,0,1}}, V[2], V[3]}, indices={1,2,3}}",
        "{vertices={{position={0,0,0}, normal={0,0,1}, typo=1}, V[2], V[3]}, indices={1,2,3}}",
    ] {
        assert!(
            lua.load(format!("return gfx.mesh({bad})"))
                .eval::<AnyUserData>()
                .is_err(),
            "{bad}"
        );
    }
    let err = lua
        .load(
            r#"
        return gfx.scene3d({camera={eye={0,0,5},target={0,0,0}},
            objects={{id="a",mesh=gfx.solid("red"),color="red"}}})
    "#,
        )
        .eval::<AnyUserData>()
        .unwrap_err();
    assert!(
        err.to_string().contains("object.mesh must be a gfx.mesh"),
        "{err}"
    );
}

#[test]
fn live_mesh_byte_budget_rejects_before_another_native_payload_is_allocated() {
    let lua = lua();
    lua.load(
        r#"
        for i=4,65536 do V[i] = V[1] end
        IX = {}
        for i=1,196608 do IX[i] = (i-1)%3+1 end
        held = {}
        for i=1,17 do held[i] = gfx.mesh({vertices=V,indices=IX}) end
    "#,
    )
    .exec()
    .unwrap();
    let err = lua
        .load("return gfx.mesh({vertices=V,indices=IX})")
        .eval::<AnyUserData>()
        .unwrap_err();
    assert!(err.to_string().contains("32 MiB budget"), "{err}");
}

#[test]
fn live_mesh_budget_is_reclaimed_after_unreferenced_resources_are_collected() {
    let lua = lua();
    lua.load("held = {}; for i=1,128 do held[i] = make() end")
        .exec()
        .unwrap();
    let err = lua.load("return make()").eval::<AnyUserData>().unwrap_err();
    assert!(err.to_string().contains("128-mesh"), "{err}");
    lua.load(
        r#"
        local objects = {}
        for i=1,128 do objects[i] = {id=tostring(i),mesh=held[i],color='red'} end
        scene = gfx.scene3d({camera={eye={0,0,5},target={0,0,0}},objects=objects})
        held = nil
    "#,
    )
    .exec()
    .unwrap();
    lua.gc_collect().unwrap();
    lua.gc_collect().unwrap();
    assert!(
        lua.load("return make()").eval::<AnyUserData>().is_err(),
        "scene still retains payload"
    );
    lua.load("scene = nil").exec().unwrap();
    lua.gc_collect().unwrap();
    lua.gc_collect().unwrap();
    assert!(lua.load("return make()").eval::<AnyUserData>().is_ok());
}
