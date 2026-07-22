#pragma once

#ifndef _MSC_VER

#include <cerrno>
#include <cmath>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cwchar>
#include <new>

#ifndef _WIN32

#ifndef _countof
#define _countof(array) (sizeof(array) / sizeof((array)[0]))
#endif

#ifndef FFX_UNUSED
#define FFX_UNUSED(value) ((void)(value))
#endif

static int byro_wcscmp(const wchar_t* left, const wchar_t* right) {
    while (*left != L'\0' && *left == *right) {
        ++left;
        ++right;
    }
    if (*left == *right) {
        return 0;
    }
    return static_cast<unsigned int>(*left) < static_cast<unsigned int>(*right) ? -1 : 1;
}

#define wcscmp byro_wcscmp

template <typename Char, size_t Size>
static int byro_copy_string(Char (&destination)[Size], const Char* source) {
    if (!source || Size == 0) {
        return EINVAL;
    }
    size_t length = 0;
    while (source[length] != Char{}) {
        ++length;
    }
    if (length >= Size) {
        destination[0] = Char{};
        return ERANGE;
    }
    for (size_t index = 0; index <= length; ++index) {
        destination[index] = source[index];
    }
    return 0;
}

template <size_t Size>
static int wcscpy_s(wchar_t (&destination)[Size], const wchar_t* source) {
    return byro_copy_string(destination, source);
}

static int wcscpy_s(wchar_t* destination, size_t size, const wchar_t* source) {
    if (!destination || !source || size == 0) {
        return EINVAL;
    }
    size_t length = 0;
    while (source[length] != L'\0') {
        ++length;
    }
    if (length >= size) {
        destination[0] = L'\0';
        return ERANGE;
    }
    for (size_t index = 0; index <= length; ++index) {
        destination[index] = source[index];
    }
    return 0;
}

static int strcpy_s(char* destination, size_t size, const char* source) {
    if (!destination || !source || size == 0) {
        return EINVAL;
    }
    const size_t length = std::strlen(source);
    if (length >= size) {
        destination[0] = '\0';
        return ERANGE;
    }
    std::memcpy(destination, source, length + 1);
    return 0;
}

template <typename... Args>
static int sprintf_s(char* destination, size_t size, const char* format, Args... args) {
    const int written = std::snprintf(destination, size, format, args...);
    return written < 0 || static_cast<size_t>(written) >= size ? -1 : written;
}

static int wcstombs_s(
    size_t* converted,
    char* destination,
    size_t size,
    const wchar_t* source,
    size_t count) {
    if (!converted || !destination || !source || size == 0) {
        return EINVAL;
    }
    const size_t limit = count < size - 1 ? count : size - 1;
    size_t result = 0;
    while (result < limit && source[result] != L'\0') {
        const unsigned int code_unit = static_cast<unsigned int>(source[result]);
        destination[result] = code_unit <= 0x7f ? static_cast<char>(code_unit) : '?';
        ++result;
    }
    destination[result] = '\0';
    *converted = result + 1;
    return 0;
}

static int swprintf_s(
    wchar_t* destination,
    size_t size,
    const wchar_t* format,
    ...) {
    if (!destination || !format || size == 0) {
        return -1;
    }
    va_list arguments;
    va_start(arguments, format);
    const int written = std::vswprintf(destination, size, format, arguments);
    va_end(arguments);
    return written;
}

#endif // !_WIN32
#endif
