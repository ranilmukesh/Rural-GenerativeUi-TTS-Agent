from sqlalchemy import Column, Integer, String, Text, Float, JSON
from sqlalchemy.ext.declarative import declarative_base

Base = declarative_base()

class UserProgress(Base):
    __tablename__ = 'user_progress'
    session_id = Column(String, primary_key=True)
    points = Column(Integer, default=0)
    level = Column(String, default="Seed (Vithu)")
    badges_earned = Column(JSON, default=[])  # List of Kural IDs
    completed_challenges = Column(Integer, default=0)
    current_skill_focus = Column(String, default="Logic")
    eligible_schemes = Column(JSON, default=[]) # Real-world links
