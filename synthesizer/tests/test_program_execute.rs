// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod utilities;
use utilities::*;

use console::{
    account::PrivateKey,
    network::prelude::*,
    program::{Identifier, Value},
};
use snarkvm_synthesizer::Process;

#[test]
fn test_program_execute() {
    // Load the tests.
    let tests = load_tests::<_, ProgramTest>("./tests/program", "./expectations/program/execute");
    // Initialize a process.
    let mut process = Process::<CurrentNetwork>::load().unwrap();

    // Run each test and compare it against its corresponding expectation.
    for test in &tests {
        // Add the program into the process.
        let program = test.program();
        process.add_program(program).unwrap();

        // Initialize the RNG.
        let rng = &mut match test.randomness() {
            None => TestRng::default(),
            Some(randomness) => TestRng::fixed(randomness),
        };

        let outputs = test
            .cases()
            .iter()
            .map(|value| {
                // Extract the function name, inputs, and optional private key.
                let value = value.as_mapping().expect("expected mapping for test case");
                let function_name = Identifier::<CurrentNetwork>::from_str(
                    value
                        .get("function")
                        .expect("expected function name for test case")
                        .as_str()
                        .expect("expected string for function name"),
                )
                .expect("unable to parse function name");
                let inputs = value
                    .get("inputs")
                    .expect("expected inputs for test case")
                    .as_sequence()
                    .expect("expected sequence for inputs")
                    .iter()
                    .map(|input| {
                        Value::<CurrentNetwork>::from_str(input.as_str().expect("expected string for input"))
                            .expect("unable to parse input")
                    })
                    .collect_vec();
                let private_key = match value.get("private_key") {
                    Some(private_key) => PrivateKey::<CurrentNetwork>::from_str(
                        private_key.as_str().expect("expected string for private key"),
                    )
                    .expect("unable to parse private key"),
                    None => PrivateKey::new(rng).unwrap(),
                };

                // Authorize the execution.
                let authorization = process
                    .authorize::<CurrentAleo, _>(&private_key, program.id(), function_name, inputs.iter(), rng)
                    .unwrap();
                // Execute the authorization.
                let (response, _, _, _) = process.execute::<CurrentAleo, _>(authorization, rng).unwrap();
                // Extract the output.
                serde_yaml::Value::Sequence(
                    response
                        .outputs()
                        .iter()
                        .cloned()
                        .map(|output| serde_yaml::Value::String(output.to_string()))
                        .collect_vec(),
                )
            })
            .collect::<Vec<_>>();
        // Check against the expected output.
        test.check(&outputs).unwrap();
        // Save the output.
        test.save(&outputs).unwrap();
    }
}
