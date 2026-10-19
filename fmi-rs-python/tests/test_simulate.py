from pathlib import Path
from zipfile import ZipFile

import numpy as np
import pytest

import fmi_rs

# downloaded by the fmi-rs Rust tests (cargo test -p fmi-rs)
REFERENCE_FMUS = Path(__file__).parents[2] / "fmi-rs" / "tests" / "resources" / "Reference-FMUs"

if not REFERENCE_FMUS.is_dir():
    pytest.skip("Reference FMUs not found, run 'cargo test -p fmi-rs' first", allow_module_level=True)


def fmu(name: str) -> Path:
    return REFERENCE_FMUS / "3.0" / f"{name}.fmu"


@pytest.mark.parametrize(
    "kwargs",
    [
        dict(fmi_type="CoSimulation"),
        dict(fmi_type="ModelExchange", solver="Euler", step_size=1e-3),
        dict(fmi_type="ModelExchange", solver="CVode"),
    ],
    ids=["cs", "me-euler", "me-cvode"],
)
def test_bouncing_ball(kwargs):
    result = fmi_rs.simulate(fmu("BouncingBall"), stop_time=3, output_interval=0.1, **kwargs)

    assert result.dtype.names == ("time", "h", "v")
    assert result["time"][0] == 0
    assert result["time"][-1] == pytest.approx(3)
    assert result["h"][0] == pytest.approx(1)
    assert np.all(result["h"] > -0.1)


def test_default_experiment():
    result = fmi_rs.simulate(fmu("BouncingBall"))
    # DefaultExperiment: stopTime="3" stepSize="0.01"
    assert result["time"][-1] == pytest.approx(3)
    assert len(np.unique(result["time"])) == 301


def test_unzipped_fmu(tmp_path):
    with ZipFile(fmu("BouncingBall")) as zf:
        zf.extractall(tmp_path)
    result = fmi_rs.simulate(tmp_path, stop_time=1, output_interval=0.1)
    assert result["time"][-1] == pytest.approx(1)


def test_start_values():
    result = fmi_rs.simulate(fmu("BouncingBall"), stop_time=1, output_interval=0.1, start_values={"h": 2})
    assert result["h"][0] == pytest.approx(2)


def test_output_selection():
    result = fmi_rs.simulate(fmu("BouncingBall"), stop_time=1, output_interval=0.1, output=["v", "g"])
    assert result.dtype.names == ("time", "v", "g")
    assert np.all(result["g"] == pytest.approx(-9.81))


def test_array_variables():
    result = fmi_rs.simulate(fmu("StateSpace"), stop_time=1, output_interval=0.1, output=["u", "y"])
    assert result["y"].shape == (len(result), 3)
    assert result["u"][0].tolist() == [1, 2, 3]


def test_input_file(tmp_path):
    input_file = tmp_path / "input.csv"
    input_file.write_text("time,Float64_continuous_input\n0,0\n1,2\n")

    result = fmi_rs.simulate(
        fmu("Feedthrough"),
        fmi_type="CoSimulation",
        stop_time=1,
        output_interval=0.5,
        input_file=input_file,
        output=["Float64_continuous_output"],
    )

    # the input is applied at the beginning of each communication step
    assert result["Float64_continuous_output"].tolist() == pytest.approx([0, 0, 1])


def test_step_finished_progress_and_stop():
    times = []

    def step_finished(time, row):
        assert row[0] == time
        times.append(time)
        return time < 1

    result = fmi_rs.simulate(fmu("BouncingBall"), stop_time=3, output_interval=0.1, step_finished=step_finished)

    assert times == result["time"].tolist()
    assert 1 <= result["time"][-1] < 1.1 + 1e-9


def test_step_finished_exception():
    def step_finished(time, row):
        raise ValueError("boom")

    with pytest.raises(ValueError, match="boom"):
        fmi_rs.simulate(fmu("BouncingBall"), stop_time=1, output_interval=0.1, step_finished=step_finished)


def test_fmi_call_logger():
    calls = []
    fmi_rs.simulate(fmu("BouncingBall"), stop_time=1, output_interval=0.1, fmi_call_logger=calls.append)
    assert any(c.startswith("fmi3InstantiateCoSimulation(") for c in calls)
    assert any(c.startswith("fmi3DoStep(") for c in calls)


def test_errors():
    with pytest.raises(fmi_rs.FmiRsError, match="Unknown solver"):
        fmi_rs.simulate(fmu("BouncingBall"), fmi_type="ModelExchange", solver="RK4")

    with pytest.raises(fmi_rs.FmiRsError, match="divisor"):
        fmi_rs.simulate(fmu("BouncingBall"), stop_time=1, output_interval=0.3)

    with pytest.raises(fmi_rs.FmiRsError):
        fmi_rs.simulate(fmu("BouncingBall"), output=["no_such_variable"])
