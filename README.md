# fmi-rs

Functional Mock-up Interface for Rust.

## Highlights

- Supports FMI 2.0 and 3.0.
- Supports Model Exchange and Co-Simulation.
- Validates XML files.
- Reads and writes CSVs.
- Provides optional SUNDIALS solvers for Model Exchange simulations.
- Generates CMake projects for source-code FMUs.

## Layered standards

FMI 3.0 [layered standards](https://fmi-standard.org/docs/3.0/#fmi-layered-standards) supported by fmi-rs:

- [fmi-ls-dae](https://github.com/modelica/fmi-ls-dae) &mdash; Differential Algebraic Equations
- [fmi-ls-wasm](https://github.com/modelica/fmi-ls-dae) &mdash; WebAssembly (experimental)

## License

fmi-rs is released under the [BSD-2-Clause license](LICENSE).

<div align="center">
    <a target="_blank" href="https://3ds.com/" style="background:none">
        <img src="https://raw.githubusercontent.com/CATIA-Systems/fmi-rs/main/assets/made-by-3ds.svg" alt="Made by 3DS">
    </a>
</div>
