#include "gamebyro-cxx-bridge/cpp/native_utils.h"

namespace gamebyro {

rust::String native_hello() {
    return rust::String("Hello from C++ side of Gamebyro Redux!");
}

} // namespace gamebyro
