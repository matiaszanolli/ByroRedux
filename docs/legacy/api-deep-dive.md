# Gamebryo 2.3 API Deep Dive

Detailed analysis of the core classes, their members, and how they map to Redux.

## NiFixedString — String Interning

```cpp
class NiFixedString : public NiMemObject {
    NiGlobalStringTable::GlobalStringHandle m_kHandle;
public:
    NiFixedString();
    NiFixedString(const char*);
    operator const char*();      // implicit conversion
    bool Exists() const;
    size_t GetLength() const;
    unsigned int GetRefCount() const;
    bool Equals(const char*) const;
    bool EqualsNoCase(const char*) const;
    bool Contains(const char*) const;
    bool ContainsNoCase(const char*) const;
};
```

**Redux equivalent:** `StringPool` + `FixedString` (already implemented). Their handle-based
approach is essentially what `string_interner::DefaultSymbol` gives us — integer comparison
for equality, O(1). We're already aligned here.

**Difference:** Gamebryo's NiFixedString has implicit `operator const char*()` conversion.
Our FixedString requires explicit `pool.resolve(sym)`. This is intentionally safer —
no dangling pointer risk.

---

## NiTransform — Transform Data

```cpp
class NiTransform : public NiMemObject {
    NiMatrix3 m_Rotate;    // 3x3 rotation matrix
    NiPoint3 m_Translate;  // translation vector
    float m_fScale;        // uniform scale

    void MakeIdentity();
    NiTransform operator*(const NiTransform&);  // composition
    NiPoint3 operator*(const NiPoint3&);         // transform point
    void Invert(NiTransform& dest);
    static void Interpolate(float t, const NiTransform& a, const NiTransform& b, NiTransform& result);
};
```

**Redux mapping:** This maps directly to a `Transform` component using glam types:
```rust
struct Transform {
    translation: Vec3,
    rotation: Quat,    // quaternion instead of matrix — more compact, SLERP-friendly
    scale: f32,        // uniform scale (same as Gamebryo)
}
```

**Key decision:** Gamebryo stores rotation as a 3x3 matrix (36 bytes). We should use
quaternion (16 bytes) — more compact, better for interpolation, standard in modern engines.
Convert on NIF load.

---

## NiObject — Root Base Class

```cpp
class NiObject : public NiRefObject {
    NiDeclareRootRTTI(NiObject);
    NiDeclareClone(NiObject);
    NiDeclareStream;

    virtual ~NiObject();
    NiObject* Clone();
    NiObject* Clone(NiCloningProcess&);
    void CreateSharedClone(NiCloningProcess&);
    NiObject* CreateDeepCopy();

    // Streaming
    virtual bool PostLinkObject(NiStream&);
    virtual bool StreamCanSkip();
    virtual const NiRTTI* GetStreamableRTTI() const;
    virtual unsigned int GetBlockAllocationSize() const;
};
```

**Redux equivalent:** No single base class needed. In ECS:
- RTTI → Rust's `TypeId` + `Component` trait
- Ref counting → Rust ownership / `Arc<T>`
- Cloning → `Clone` trait
- Streaming → NIF parser produces entities + components

---

## NiAVObject — Core Game Object

The most important class. Everything visible in the scene graph inherits from this.

```cpp
class NiAVObject : public NiObjectNET {
protected:
    NiTransform m_kLocal;           // local-space transform
    NiTransform m_kWorld;           // world-space transform
    NiNode* m_pkParent;             // parent in scene graph
    NiBound m_kWorldBound;          // bounding sphere
    NiPropertyList m_kPropertyList; // attached properties
    NiCollisionObjectPtr m_spCollisionObject;
    unsigned short m_uFlags;        // packed bitflags

public:
    // Transform access
    void SetTranslate(const NiPoint3&);
    const NiPoint3& GetTranslate() const;
    void SetRotate(const NiMatrix3&);
    const NiMatrix3& GetRotate() const;
    void SetScale(float);
    float GetScale() const;

    // World-space (read-only from user perspective)
    const NiPoint3& GetWorldTranslate() const;
    const NiMatrix3& GetWorldRotate() const;
    float GetWorldScale() const;
    const NiBound& GetWorldBound() const;

    // Properties
    void AttachProperty(NiProperty*);
    void DetachProperty(NiProperty*);
    NiProperty* GetProperty(int type);

    // Hierarchy
    NiNode* GetParent();
    NiAVObject* GetObjectByName(const NiFixedString&);

    // Collision
    void SetCollisionObject(NiCollisionObject*);
    NiCollisionObject* GetCollisionObject();

    // Update cascade
    void Update(float fTime, bool bUpdateControllers);
    void UpdateProperties();
    void UpdateEffects();

    // Culling
    void SetAppCulled(bool);
    bool GetAppCulled() const;

    // Flags (packed into unsigned short)
    // APP_CULLED      = 0x0001
    // SELECTIVE_*     = 0x0002-0x0080
    // IS_NODE         = 0x0100
};
```

**Redux ECS decomposition:**

| NiAVObject field | Redux Component | Storage |
|---|---|---|
| m_kLocal (transform) | `LocalTransform` | PackedStorage (hot) |
| m_kWorld (world transform) | `WorldTransform` | PackedStorage (hot) |
| m_pkParent | `Parent(EntityId)` | SparseSetStorage |
| children (in NiNode) | `Children(Vec<EntityId>)` | SparseSetStorage |
| m_kWorldBound | `WorldBound` | PackedStorage |
| m_kPropertyList | Individual property components | Varies |
| m_spCollisionObject | `CollisionObject` | SparseSetStorage |
| m_uFlags | `SceneFlags` | PackedStorage (read every frame) |
| Name (from NiObjectNET) | `Name(FixedString)` | SparseSetStorage (already done) |

**Key insight:** NiAVObject is a God Object. In ECS, we decompose it into ~8 independent
components. Systems that only need transforms don't touch collision data. Systems that
only need bounds don't touch properties. This is the whole point of the architecture change.

---

## NiProperty — Render State

```cpp
class NiProperty : public NiObjectNET {
public:
    enum {
        ALPHA = 0,    DITHER = 1,    FOG = 2,
        MATERIAL = 3, REND_SPEC = 4, SHADE = 5,
        SPECULAR = 6, STENCIL = 7,   TEXTURING = 8,
        VERTEX_COLOR = 9, WIREFRAME = 10, ZBUFFER = 11,
        MAX_TYPES = 12
    };

    virtual int Type() const = 0;
    virtual void Update(float fTime);
};
```

**Redux approach:** Properties become components or parts of a unified `Material` component:

```
NiAlphaProperty     → AlphaBlend component or material field
NiTexturingProperty → TextureBindings component (maps to Vulkan descriptor sets)
NiMaterialProperty  → MaterialColors component
NiZBufferProperty   → DepthState (part of pipeline state)
NiStencilProperty   → StencilState (part of pipeline state)
```

Most of these map to Vulkan pipeline state objects rather than per-object components.

---

## NiStream — NIF File Format

```cpp
class NiStream : public NiMemObject {
    // Version tracking
    unsigned int m_uiNifFileVersion;
    unsigned int m_uiNifFileUserDefinedVersion;

    // I/O
    NiBinaryStream* m_pkIstr;
    NiBinaryStream* m_pkOstr;

    // Object registry
    NiTPointerMap<const NiObject*, unsigned int> m_kRegisterMap;

    // Error handling
    enum {
        STREAM_OKAY, FILE_NOT_LOADED, NOT_NIF_FILE,
        OLDER_VERSION, LATER_VERSION, NO_CREATE_FUNCTION,
        ENDIAN_MISMATCH
    };

    static const unsigned int NULL_LINKID = 0xffffffff;

    // Loading
    bool Load(const char* filename);
    bool Load(char* buffer, int size);
    bool Load(NiBinaryStream*);

    // Background loading
    enum ThreadStatus { IDLE, LOADING, CANCELLING, PAUSING, PAUSED };
    void BackgroundLoadBegin(const char* filename);
    ThreadStatus BackgroundLoadPoll(LoadState*);

    // Internal phases
    void LoadHeader();
    void LoadStream();
    void LoadRTTI();
    void LoadFixedStringTable();
    void LoadObjectSizeTable();
    void LoadObjectGroups();

    // Link resolution
    unsigned int ReadLinkID();
    NiObject* ResolveLinkID();
    NiObject* GetObjectFromLinkID(unsigned int);
};
```

**Redux NIF loader design (future crate):**

The NIF loader should be a separate crate (`crates/nif/`) that:
1. Reads the binary format (header → objects → RTTI → strings → sizes → groups)
2. Resolves link IDs between objects
3. Produces ECS entities with components (Transform, Mesh, Material, Name, etc.)
4. No NiObject hierarchy — directly into flat ECS storage

Three-phase approach mirrors Gamebryo:
1. **Parse** — read binary blocks, build raw NIF object list
2. **Link** — resolve cross-references between objects
3. **Import** — convert NIF objects into ECS entities + components

---

## Update Pattern: Gamebryo vs Redux

**Gamebryo:**
```
NiNode::Update(time)
  → UpdateDownwardPass()     // parent → children transforms
  → UpdateUpwardPass()       // children → parent bounds
  → UpdateControllers(time)  // animation
  → UpdateProperties()       // material state
  → UpdateEffects()          // lights
```

**Redux (systems in scheduler):**
```
animation_system(world, dt)     // update interpolators → write transforms
transform_system(world, dt)     // propagate local → world transforms
bounds_system(world, dt)        // recompute world bounds
property_system(world, dt)      // update material state (if animated)
cull_system(world, dt)          // frustum culling
render_system(world, dt)        // submit visible geometry
```

Each system touches only the components it needs. No God Object update cascade.
