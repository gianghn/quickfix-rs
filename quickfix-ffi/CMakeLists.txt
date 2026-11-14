cmake_minimum_required(VERSION 3.10)
project(QuickFixBind)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED True)
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}/libquickfix/cmake/")

option(HAVE_SSL "Build with SSL" OFF)
option(HAVE_MYSQL "Build with MySQL" OFF)
option(HAVE_POSTGRESQL "Build with PostgreSQL" OFF)

option(QUICKFIX_BIND_EXAMPLES "Build quickfix C binding examples" ON)

# KRX fork: the house C++ policy in fork mode (.cpp-policy-fork). Its checker passes HOUSE_POLICY_DIR;
# build.rs doesn't, so cargo builds are unchanged.
set(HOUSE_FORK ON)
if(HOUSE_POLICY_DIR)
    # HouseFlags checks PIE support, which needs this policy; cmake_minimum_required above predates it.
    cmake_policy(SET CMP0083 NEW)
    include("${HOUSE_POLICY_DIR}/HouseFlags.cmake")
    # build.rs takes <quickfix/*.h> from libquickfix's install. Copy them at configure time instead, so the
    # policy's clang-tidy (configure only) sees the real types without building libquickfix.
    file(GLOB quickfix_headers "${CMAKE_CURRENT_SOURCE_DIR}/libquickfix/src/C++/*.h")
    file(COPY ${quickfix_headers} DESTINATION "${CMAKE_BINARY_DIR}/quickfix-include/quickfix")
    include_directories(SYSTEM "${CMAKE_BINARY_DIR}/quickfix-include")
    # shortcut: the examples link libquickfix, which the policy build doesn't build; check them if they change.
    set(QUICKFIX_BIND_EXAMPLES OFF)
endif()

if (HAVE_SSL)
    find_package(OpenSSL REQUIRED)
    include_directories(${OPENSSL_INCLUDE_DIR})
    message("-- Building with OpenSSL")
endif()

if (HAVE_MYSQL)
    find_package(MySQL REQUIRED)
    include_directories(${MYSQL_INCLUDE_DIR})
    message("-- Building with MySQL")
endif()

if (HAVE_POSTGRESQL)
    find_package(PostgreSQL REQUIRED)
    include_directories(${PostgreSQL_INCLUDE_DIRS})
    message("-- Building with POSTGRESQL")
endif()

# Add main directory
add_subdirectory(quickfix-bind)

# Add example if asked
if(QUICKFIX_BIND_EXAMPLES)
add_subdirectory(examples)
endif()