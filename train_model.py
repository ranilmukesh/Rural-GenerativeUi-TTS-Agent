import pandas as pd
import numpy as np
import joblib
import xgboost as xgb
import shap
import networkx as nx
from collections import Counter
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import LabelEncoder, StandardScaler
from sklearn.impute import SimpleImputer
from sklearn.pipeline import Pipeline
from sklearn.ensemble import StackingClassifier, RandomForestClassifier, GradientBoostingClassifier
from sklearn.linear_model import LogisticRegression
from imblearn.over_sampling import SMOTE
import warnings
warnings.filterwarnings('ignore')

from routing_engine import RoutingEngine

def main():
    print("=" * 60)
    print("    Placement Predictor & Career Router Training")
    print("=" * 60)
    
    # Configuration
    PLACE_DATA = "collegePlace.csv"
    CAREER_DATA = "Tech_Data_Cleaned.csv"
    OUTPUT_PATH = "placement_artifacts.pkl"
    
    print("[*] Loading placement dataset...")
    df = pd.read_csv(PLACE_DATA)
    
    # Preprocessing
    le_gender = LabelEncoder()
    le_stream = LabelEncoder()
    
    df['Gender'] = le_gender.fit_transform(df['Gender'])
    df['Stream'] = le_stream.fit_transform(df['Stream'])
    
    X = df.drop('PlacedOrNot', axis=1)
    y = df['PlacedOrNot']
    
    feature_names = list(X.columns)
    
    print("\n[*] Balancing dataset with SMOTE...")
    smote = SMOTE(random_state=42)
    X_resampled, y_resampled = smote.fit_resample(X, y)
    
    # Train-Test Split
    X_train, X_test, y_train, y_test = train_test_split(
        X_resampled, y_resampled, test_size=0.2, random_state=42
    )
    
    print("\n[*] Building Preprocessor Pipeline...")
    preprocessor = Pipeline([
        ('imputer', SimpleImputer(strategy='median')),
        ('scaler', StandardScaler())
    ])

    X_train_processed = preprocessor.fit_transform(X_train)
    X_test_processed = preprocessor.transform(X_test)

    print("\n[*] Training StackingClassifier Ensemble...")
    estimators = [
        ('rf', RandomForestClassifier(n_estimators=100, max_depth=5, random_state=42)),
        ('gbc', GradientBoostingClassifier(n_estimators=100, learning_rate=0.1, random_state=42)),
        ('xgb', xgb.XGBClassifier(use_label_encoder=False, eval_metric='logloss', max_depth=4))
    ]

    stacking_model = StackingClassifier(
        estimators=estimators,
        final_estimator=LogisticRegression(),
        cv=5,
        stack_method='predict_proba'
    )
    stacking_model.fit(X_train_processed, y_train)

    accuracy = stacking_model.score(X_test_processed, y_test)
    print(f"    [OK] Ensemble Training complete! Accuracy: {accuracy:.4f}")
    
    print("\n[*] Training Standalone XGBoost for SHAP...")
    standalone_xgb = xgb.XGBClassifier(use_label_encoder=False, eval_metric='logloss', max_depth=4)
    standalone_xgb.fit(X_train_processed, y_train)
    
    print("\n[*] Initializing RoutingEngine from Career Data...")
    routing_engine = RoutingEngine(CAREER_DATA)
    print(f"    [OK] Extracted {len(routing_engine.all_jobs)} jobs and {len(routing_engine.all_unique_skills)} skills.")

    print(f"\n[*] Saving artifacts to '{OUTPUT_PATH}'...")
    artifacts = {
        'preprocessor': preprocessor,
        'model': stacking_model,
        'shap_model': standalone_xgb,
        'le_gender': le_gender,
        'le_stream': le_stream,
        'routing_engine': routing_engine
    }
    joblib.dump(artifacts, OUTPUT_PATH)
    print("    [OK] Successfully saved!")
    print("=" * 60)

if __name__ == "__main__":
    main()
