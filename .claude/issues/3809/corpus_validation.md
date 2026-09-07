# Havok packfile decoder — corpus validation

| check | blobs | share |
|---|---:|---:|
| blobs examined | 4484 | — |
| `parse_havok_packfile` succeeds | 4484 | 100.0% |
| last section `absolute_end()` == blob length | 4484 | 100.0% |
| **every virtual fixup resolves to a declared class** | **4484** | **100.0%** |
| objects in strictly ascending offset order | 4484 | 100.0% |
| every global fixup names a real section | 4484 | 100.0% |
| every local fixup lands inside the section | 4484 | 100.0% |

## Fixup-table sizes

| table | min | median | max |
|---|---:|---:|---:|
| global | 5 | 5 | 5 |
| local | 14 | 54 | 4098 |
| virtual | 5 | 5 | 5 |

## SDK version string

| value | blobs |
|---|---:|
| `hk_2014.1.0-r1` | 4484 |

## Object sets found in `__data__`

| objects, in file order | blobs |
|---|---:|
| hknpPhysicsSystemData + hknpCompressedMeshShape + hkRefCountedProperties + hknpBSMaterialProperties + hknpCompressedMeshShapeData | 4484 |

## Classes declared

| class | blobs |
|---|---:|
| `hkClass` | 4484 |
| `hkClassEnum` | 4484 |
| `hkClassEnumItem` | 4484 |
| `hkClassMember` | 4484 |
| `hkRefCountedProperties` | 4484 |
| `hknpBSMaterialProperties` | 4484 |
| `hknpCompressedMeshShape` | 4484 |
| `hknpCompressedMeshShapeData` | 4484 |
| `hknpPhysicsSystemData` | 4484 |
