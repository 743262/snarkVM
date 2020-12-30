// Copyright (C) 2019-2020 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use algebra::UniformRand;
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;

use algebra::{
    biginteger::BigInteger832 as FqRepr,
    fields::sw6::fq::Fq,
    BigInteger,
    Field,
    PrimeField,
    SquareRootField,
};
use std::ops::{AddAssign, MulAssign, SubAssign};

#[bench]
fn bench_fq_repr_add_nocarry(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<(FqRepr, FqRepr)> = (0..SAMPLES)
        .map(|_| {
            let mut tmp1 = FqRepr::rand(&mut rng);
            let mut tmp2 = FqRepr::rand(&mut rng);
            // Shave a few bits off to avoid overflow.
            for _ in 0..3 {
                tmp1.div2();
                tmp2.div2();
            }
            (tmp1, tmp2)
        })
        .collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count].0;
        tmp.add_nocarry(&v[count].1);
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_repr_sub_noborrow(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<(FqRepr, FqRepr)> = (0..SAMPLES)
        .map(|_| {
            let tmp1 = FqRepr::rand(&mut rng);
            let mut tmp2 = tmp1;
            // Ensure tmp2 is smaller than tmp1.
            for _ in 0..10 {
                tmp2.div2();
            }
            (tmp1, tmp2)
        })
        .collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count].0;
        tmp.sub_noborrow(&v[count].1);
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_repr_num_bits(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<FqRepr> = (0..SAMPLES).map(|_| FqRepr::rand(&mut rng)).collect();

    let mut count = 0;
    b.iter(|| {
        let tmp = v[count].num_bits();
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_repr_mul2(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<FqRepr> = (0..SAMPLES).map(|_| FqRepr::rand(&mut rng)).collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count];
        tmp.mul2();
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_repr_div2(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<FqRepr> = (0..SAMPLES).map(|_| FqRepr::rand(&mut rng)).collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count];
        tmp.div2();
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_add_assign(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<(Fq, Fq)> = (0..SAMPLES).map(|_| (Fq::rand(&mut rng), Fq::rand(&mut rng))).collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count].0;
        tmp.add_assign(&v[count].1);
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_sub_assign(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<(Fq, Fq)> = (0..SAMPLES).map(|_| (Fq::rand(&mut rng), Fq::rand(&mut rng))).collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count].0;
        tmp.sub_assign(&v[count].1);
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_mul_assign(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<(Fq, Fq)> = (0..SAMPLES).map(|_| (Fq::rand(&mut rng), Fq::rand(&mut rng))).collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count].0;
        tmp.mul_assign(&v[count].1);
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_double(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<Fq> = (0..SAMPLES).map(|_| Fq::rand(&mut rng)).collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count];
        tmp.double_in_place();
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_square(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<Fq> = (0..SAMPLES).map(|_| Fq::rand(&mut rng)).collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count];
        tmp.square_in_place();
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_inverse(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<Fq> = (0..SAMPLES).map(|_| Fq::rand(&mut rng)).collect();

    let mut count = 0;
    b.iter(|| {
        count = (count + 1) % SAMPLES;
        v[count].inverse()
    });
}

#[bench]
fn bench_fq_negate(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<Fq> = (0..SAMPLES).map(|_| Fq::rand(&mut rng)).collect();

    let mut count = 0;
    b.iter(|| {
        let mut tmp = v[count];
        tmp = -tmp;
        count = (count + 1) % SAMPLES;
        tmp
    });
}

#[bench]
fn bench_fq_sqrt(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<Fq> = (0..SAMPLES)
        .map(|_| {
            let mut tmp = Fq::rand(&mut rng);
            tmp.square_in_place();
            tmp
        })
        .collect();

    let mut count = 0;
    b.iter(|| {
        count = (count + 1) % SAMPLES;
        v[count].sqrt()
    });
}

#[bench]
fn bench_fq_into_repr(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<Fq> = (0..SAMPLES).map(|_| Fq::rand(&mut rng)).collect();

    let mut count = 0;
    b.iter(|| {
        count = (count + 1) % SAMPLES;
        v[count].into_repr()
    });
}

#[bench]
fn bench_fq_from_repr(b: &mut ::test::Bencher) {
    const SAMPLES: usize = 1000;

    let mut rng = XorShiftRng::seed_from_u64(1231275789u64);

    let v: Vec<FqRepr> = (0..SAMPLES).map(|_| Fq::rand(&mut rng).into_repr()).collect();

    let mut count = 0;
    b.iter(|| {
        count = (count + 1) % SAMPLES;
        Fq::from_repr(v[count])
    });
}
