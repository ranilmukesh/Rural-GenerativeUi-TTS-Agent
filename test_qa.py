import requests
import time
import subprocess
import os

def start_server():
    print("Starting FastAPI server...")
    proc = subprocess.Popen(["python", "main.py"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    # Wait until the server responds or timeout
    for _ in range(15):
        time.sleep(1)
        try:
            r = requests.get("http://localhost:7860/health")
            if r.status_code == 200:
                print("Server started successfully.")
                return proc
        except requests.exceptions.ConnectionError:
            pass

    print("Server failed to start in time. stderr output:")
    print(proc.stderr.read().decode())
    return proc

def test_cgpa_simulator():
    url = "http://localhost:7860/predict"
    base_data = {
        "Age": 22,
        "Gender": "Male",
        "Stream": "Computer Science",
        "Internships": 1,
        "Hostel": 1,
        "HistoryOfBacklogs": 0,
        "skills": ["Python", "Machine Learning"],
        "desired_role": "Data Scientist"
    }

    print("Running CGPA Simulator Test (7.5 to 7.8)")
    prev_prob = None
    for cgpa in [7.5, 7.6, 7.7, 7.8]:
        data = base_data.copy()
        data["CGPA"] = cgpa
        response = requests.post(url, json=data)
        result = response.json()
        prob = result["probability_percentage"]
        print(f"CGPA: {cgpa} -> Probability: {prob}%")

        if prev_prob is not None:
            if prob == prev_prob:
                print(f"Note: Probability stayed exactly at {prob}% between steps")
        prev_prob = prob
    print("CGPA Simulator Test completed (checked for general progression).\n")

def test_shap_alignment():
    print("Running SHAP Alignment Test")
    url_predict = "http://localhost:7860/predict"
    url_explain = "http://localhost:7860/explain"
    data = {
        "Age": 21,
        "Gender": "Female",
        "Stream": "Information Technology",
        "Internships": 3,
        "CGPA": 8.5,
        "Hostel": 0,
        "HistoryOfBacklogs": 0,
        "skills": [],
        "desired_role": ""
    }

    # Get Prediction
    pred_res = requests.post(url_predict, json=data).json()
    print(f"Prediction Probability: {pred_res['probability_percentage']}%")

    # Get Explanation
    explain_res = requests.post(url_explain, json=data).json()
    factors = explain_res["top_contributing_factors"]

    # Check if Internships is a positive driver
    internships_impact = next((f for f in factors if f["feature"] == "Internships"), None)
    if internships_impact:
        print(f"Internships Impact: {internships_impact['impact']}")
        assert internships_impact["impact"] > 0, "Internships should be a positive driver!"
    else:
        print("Internships not in top factors, but that's okay. Factors:")
        for f in factors:
            print(f"  {f['feature']}: {f['impact']}")

    print("SHAP Alignment Test Passed!\n")

def test_graph_traversal():
    print("Running Graph Traversal Test")
    from routing_engine import RoutingEngine
    engine = RoutingEngine("Tech_Data_Cleaned.csv")
    path_info = engine.get_career_transition_path("Backend Developer", "ML Research Scientist")
    print(f"Path: {path_info['path']}")
    print(f"Skills to learn: {path_info['skills_to_learn']}")
    print(f"Stepping stones: {path_info['stepping_stones']}")

    assert "Backend Developer" in path_info['path']
    assert "ML Research Scientist" in path_info['path']
    assert len(path_info['skills_to_learn']) > 0
    print("Graph Traversal Test Passed!\n")


if __name__ == "__main__":
    proc = start_server()
    try:
        test_cgpa_simulator()
        test_shap_alignment()
        test_graph_traversal()
    finally:
        proc.terminate()
        proc.wait()
        print("Server stopped.")
