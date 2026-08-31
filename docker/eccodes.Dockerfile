FROM rust:1.91.0-bookworm AS eccodes-builder

ARG LIBAEC_VERSION=1.1.7
ARG LIBAEC_SHA256=26661a569a7def45a2e97fbbd09e0dc5bbb2f8ab1b41250c19e795559eec6fb2
ARG ECBUILD_VERSION=3.12.0
ARG ECBUILD_SHA256=70c7fc9b17f736a3312167c2c36d13b3b5833a255fe2b168b2886ad7c743ffdf
ARG ECCODES_VERSION=2.47.0
ARG ECCODES_SHA256=6318a6a1e296d6698bcd83719212cf630b9bb45c054723c22bbc449090ae0c0e

RUN echo "deb http://deb.debian.org/debian bookworm-backports main" \
        > /etc/apt/sources.list.d/bookworm-backports.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        cmake/bookworm-backports \
        curl \
        libopenjp2-7-dev \
        libpng-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN curl --fail --location --show-error --silent \
        "https://github.com/Deutsches-Klimarechenzentrum/libaec/archive/refs/tags/v${LIBAEC_VERSION}.tar.gz" \
        --output /tmp/libaec.tar.gz \
    && echo "${LIBAEC_SHA256}  /tmp/libaec.tar.gz" | sha256sum --check - \
    && tar --extract --gzip --file /tmp/libaec.tar.gz --directory /tmp \
    && cmake \
        -S "/tmp/libaec-${LIBAEC_VERSION}" \
        -B /tmp/libaec-build \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX=/usr/local \
        -DBUILD_TESTING=OFF \
        -DBUILD_SHARED_LIBS=ON \
        -DBUILD_STATIC_LIBS=ON \
    && cmake --build /tmp/libaec-build --parallel 2 \
    && cmake --install /tmp/libaec-build

RUN curl --fail --location --show-error --silent \
        "https://github.com/ecmwf/ecbuild/archive/refs/tags/${ECBUILD_VERSION}.tar.gz" \
        --output /tmp/ecbuild.tar.gz \
    && echo "${ECBUILD_SHA256}  /tmp/ecbuild.tar.gz" | sha256sum --check - \
    && tar --extract --gzip --file /tmp/ecbuild.tar.gz --directory /tmp \
    && curl --fail --location --show-error --silent \
        "https://github.com/ecmwf/eccodes/archive/refs/tags/${ECCODES_VERSION}.tar.gz" \
        --output /tmp/eccodes.tar.gz \
    && echo "${ECCODES_SHA256}  /tmp/eccodes.tar.gz" | sha256sum --check - \
    && tar --extract --gzip --file /tmp/eccodes.tar.gz --directory /tmp

RUN cmake \
    -S "/tmp/eccodes-${ECCODES_VERSION}" \
    -B /tmp/eccodes-build \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr/local \
    -Decbuild_DIR="/tmp/ecbuild-${ECBUILD_VERSION}/cmake" \
    -DENABLE_AEC=ON \
    -DENABLE_BUILD_TOOLS=OFF \
    -DENABLE_EXAMPLES=OFF \
    -DENABLE_FORTRAN=OFF \
    -DENABLE_GEOGRAPHY=OFF \
    -DENABLE_JPG=ON \
    -DENABLE_JPG_LIBJASPER=OFF \
    -DENABLE_JPG_LIBOPENJPEG=ON \
    -DENABLE_NETCDF=OFF \
    -DENABLE_PNG=ON \
    -DENABLE_PRODUCT_BUFR=OFF \
    -DENABLE_PRODUCT_GRIB=ON \
    -DENABLE_TESTS=OFF \
    -DENABLE_USE_SHARED_LIB_AEC=ON

RUN cmake --build /tmp/eccodes-build --parallel 2 \
    && cmake --install /tmp/eccodes-build

FROM rust:1.91.0-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        libopenjp2-7 \
        libpng16-16 \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY --from=eccodes-builder /usr/local /usr/local

RUN ldconfig \
    && test -f /usr/local/include/libaec.h \
    && test -f /usr/local/lib/libaec.so.0.1.7 \
    && test "$(pkg-config --modversion eccodes)" = "2.47.0"

WORKDIR /workspace
ENV CARGO_TERM_COLOR=always
