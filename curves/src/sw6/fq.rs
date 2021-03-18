// Copyright (C) 2019-2021 Aleo Systems Inc.
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

use snarkvm_fields::{FieldParameters, Fp832, Fp832Parameters};
use snarkvm_utilities::biginteger::BigInteger832 as BigInteger;

pub type Fq = Fp832<FqParameters>;

pub struct FqParameters;

impl Fp832Parameters for FqParameters {}

impl FieldParameters for FqParameters {
    type BigInteger = BigInteger;

    const CAPACITY: u32 = Self::MODULUS_BITS - 1;
    /// GENERATOR = 13
    const GENERATOR: BigInteger = BigInteger([
        16669393626057438558u64,
        1640520694378723217u64,
        1598646156981121135u64,
        12401834967100173388u64,
        2356467520877704673u64,
        14759118825104212161u64,
        5556628239575210651u64,
        5317520392768798654u64,
        16398429955031064995u64,
        3556102264904210145u64,
        8166834915717907988u64,
        11926665585800594452u64,
        11716u64,
    ]);
    const INV: u64 = 14469047335842394791u64;
    /// MODULUS = 22369874298875696930346742206501054934775599465297184582183496627646774052458024540232479018147881220178054575403841904557897715222633333372134756426301062487682326574958588001132586331462553235407484089304633076250782629492557320825577
    const MODULUS: BigInteger = BigInteger([
        0xdace79b57b942ae9,
        0x545d85c16dfd424a,
        0xee135c065f4d26b7,
        0x9c2f764a12c4024b,
        0x1ad533049cfe6a39,
        0x52a3fb77c79c1320,
        0xab3596c8617c5792,
        0x830c728d80f9d78b,
        0x6a7223ee72023d07,
        0xbc5d176b746af026,
        0xe959283d8f526663,
        0xc4d2263babf8941f,
        0x3848,
    ]);
    const MODULUS_BITS: u32 = 782;
    const MODULUS_MINUS_ONE_DIV_TWO: BigInteger = BigInteger([
        0x6d673cdabdca1574,
        0xaa2ec2e0b6fea125,
        0xf709ae032fa6935b,
        0xce17bb2509620125,
        0xd6a99824e7f351c,
        0x2951fdbbe3ce0990,
        0xd59acb6430be2bc9,
        0xc1863946c07cebc5,
        0x353911f739011e83,
        0xde2e8bb5ba357813,
        0xf4ac941ec7a93331,
        0x6269131dd5fc4a0f,
        0x1c24,
    ]);
    const R: BigInteger = BigInteger([
        11190988450819017841u64,
        16170411717126802030u64,
        2265463223430229059u64,
        16946880912571045974u64,
        11155248462028513229u64,
        12855672356664541314u64,
        8489376931127408159u64,
        2655797810825538098u64,
        9648483887143916718u64,
        17514963461276738952u64,
        16777089214204267338u64,
        15649035958020076168u64,
        8659u64,
    ]);
    const R2: BigInteger = BigInteger([
        13983406830510863714u64,
        17863856572171232656u64,
        1698388424046564526u64,
        1773634430448388392u64,
        8684647957094413275u64,
        3992637317298078843u64,
        18420879196616862245u64,
        3238482510270583127u64,
        7928200707794018216u64,
        10024831010452223910u64,
        9613847725664942650u64,
        15361265984156787358u64,
        7833u64,
    ]);
    const REPR_SHAVE_BITS: u32 = 50;
    const ROOT_OF_UNITY: BigInteger = BigInteger([
        18044746167194862600u64,
        63590321303744709u64,
        5009346151370959890u64,
        2859114157767503991u64,
        8301813204852325413u64,
        5629414263664332594u64,
        2637340888701394641u64,
        17433538052687852753u64,
        2230763098934759248u64,
        3785382115983092023u64,
        8895511354022222370u64,
        15792083141709071785u64,
        1328u64,
    ]);
    // T =
    // 2796234287359462116293342775812631866846949933162148072772937078455846756557253067529059877268485152522256821925480238069737214402829166671516844553287632810960290821869823500141573291432819154425935511163079134531347828686569665103197
    const T: BigInteger = BigInteger([
        0x5b59cf36af72855d,
        0xea8bb0b82dbfa849,
        0x7dc26b80cbe9a4d6,
        0x3385eec942588049,
        0x35aa660939fcd47,
        0x4a547f6ef8f38264,
        0x7566b2d90c2f8af2,
        0xf0618e51b01f3af1,
        0xcd4e447dce4047a0,
        0x778ba2ed6e8d5e04,
        0xfd2b2507b1ea4ccc,
        0x189a44c7757f1283,
        0x709,
    ]);
    const TWO_ADICITY: u32 = 3;
    // (T - 1)/2 =
    // 1398117143679731058146671387906315933423474966581074036386468539227923378278626533764529938634242576261128410962740119034868607201414583335758422276643816405480145410934911750070786645716409577212967755581539567265673914343284832551598
    const T_MINUS_ONE_DIV_TWO: BigInteger = BigInteger([
        0xadace79b57b942ae,
        0x7545d85c16dfd424,
        0xbee135c065f4d26b,
        0x99c2f764a12c4024,
        0x1ad533049cfe6a3,
        0x252a3fb77c79c132,
        0xbab3596c8617c579,
        0x7830c728d80f9d78,
        0x66a7223ee72023d0,
        0x3bc5d176b746af02,
        0xfe959283d8f52666,
        0x8c4d2263babf8941,
        0x384,
    ]);
}
