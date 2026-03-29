#include "byroredux-cxx-bridge/cpp/native_utils.h"

namespace byroredux {

rust::String native_hello() {
    return rust::String("Hello from C++ side of ByroRedux!");
}

} // namespace byroredux
