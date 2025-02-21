/*
 *
 * nbody_CPU_AOS.h
 *
 * Scalar CPU implementation of the O(N^2) N-body calculation.
 *
 * Copyright (c) 2011-2012, Archaea Software, LLC.
 * Copyright (c) 2012-2021, Uplink Laboratories, LLC.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in
 *    the documentation and/or other materials provided with the
 *    distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
 * "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
 * LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
 * FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
 * COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
 * INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
 * BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
 * LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
 * CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN
 * ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 *
 */

#include <chrono>

#ifdef USE_CUDA
#undef USE_CUDA
#endif
#include "chCUDA.h"

#include "nbody_util.h"

#include "bodybodyInteraction.cuh"
#include "nbody_CPU_AOS.h"

using namespace std;

DEFINE_AOS(ComputeGravitation_AOS)
{
    auto start = chrono::steady_clock::now();

    ASSERT_ALIGNED(force, NBODY_ALIGNMENT);
    ASSERT_ALIGNED(posMass, NBODY_ALIGNMENT);

    ASSUME(N % 1024 == 0);
    ASSUME(N >= 1024);

    #pragma omp parallel for schedule(guided)
    for ( size_t i = 0; i < N; i++ )
    {
        float acx, acy, acz;
        const float myX = posMass[i*4+0];
        const float myY = posMass[i*4+1];
        const float myZ = posMass[i*4+2];

        acx = acy = acz = 0;

        #pragma omp simd reduction(+:acx,acy,acz)
        for ( size_t j = 0; j < N; j++ ) {

            float fx, fy, fz;
            const float bodyX = posMass[j*4+0];
            const float bodyY = posMass[j*4+1];
            const float bodyZ = posMass[j*4+2];
            const float bodyMass = posMass[j*4+3];

            bodyBodyInteraction(
                &fx, &fy, &fz,
                myX, myY, myZ,
                bodyX, bodyY, bodyZ, bodyMass,
                softeningSquared );

            acx += fx;
            acy += fy;
            acz += fz;
        }

        force[4*i+0] = acx;
        force[4*i+1] = acy;
        force[4*i+2] = acz;
    }

    auto end = chrono::steady_clock::now();
    return chrono::duration<float, std::milli>(end - start).count();
}

/* vim: set ts=4 sts=4 sw=4 et: */
