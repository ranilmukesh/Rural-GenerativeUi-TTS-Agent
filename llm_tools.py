import json
from typing import Literal, Optional, List, Union
from pydantic import BaseModel, Field, field_validator
from agno.agent import Agent
from agno.models.nvidia import Nvidia
from agno.tools import tool

class GameStep(BaseModel):
    step_type: Literal["dialogue", "choice", "sequence", "code_debug"]
    paati_says: str = Field(description="Tanglish narration from Paati")
    # optional with default so LLM omitting the field doesn't break validation
    visual: str = Field(default="🎯", description="Emoji/scene like '🥭🥭🥭👑'")
    question: Optional[str] = None
    options: Optional[List[str]] = None
    # Validator below coerces list -> str so LLM returning ["a","b"] doesn't break parsing
    correct_answer: Optional[str] = None
    sequence_items: Optional[List[str]] = Field(default=None, description="List of items in randomized order to be sorted")
    correct_sequence: Optional[List[str]] = Field(default=None, description="The correct ordered list of sequence_items")
    code_snippet: Optional[str] = Field(default=None, description="Code snippet containing a bug. Number the lines starting at 1.")
    bug_line: Optional[int] = Field(default=None, description="The exact integer line number where the bug is.")
    points_on_success: int = 10

    @field_validator("correct_answer", mode="before")
    @classmethod
    def coerce_list_to_str(cls, v):
        """LLM sometimes returns a list for correct_answer on sequence steps. Join it."""
        if isinstance(v, list):
            return ", ".join(str(x) for x in v)
        return v

class MiniGame(BaseModel):
    title: str = Field(description="e.g. 'Tenali's Mango Market Simulator'")
    concept_taught: str = Field(description="e.g. 'Compound Interest', 'SQL JOIN', 'Python Loops'")
    level: Literal["Seed", "Sapling", "Tree"]
    steps: List[GameStep] = Field(min_length=3, max_length=6)
    final_reward_badge: str

game_agent = Agent(
    model=Nvidia(id="qwen/qwen3-coder-480b-a35b-instruct", 
    max_tokens=16384,
    temperature=0.2),
    output_schema=MiniGame,
    debug_mode=True,
    use_json_mode=True,
    description="You design tiny gamified JSON lessons for rural Tamil students using Tenali Raman folklore.",
    instructions=[
        "You output ONLY one valid JSON object. No prose. No markdown. No code fences.",
        "level MUST be one of: Seed, Sapling, Tree.",
        "step_type MUST be one of: dialogue, choice, sequence, code_debug.",
        "If step_type is 'sequence', you MUST provide 'sequence_items' (random order) and 'correct_sequence' (the answer).",
        "If step_type is 'code_debug', you MUST provide 'code_snippet' (with line numbers visible in the string) and the 'bug_line' integer.",
        "steps array length: 3 to 6.",
        "Tanglish narration. Use mangoes, bullock carts, paddy field metaphors.",
    ],
)

@tool(name="generate_mini_game", description="Generate an interactive mini-game (quiz, sorting sequence, or bug hunt) teaching a concept.")
def generate_mini_game(concept: str, level: str) -> str:
    """
    Generate an interactive mini-game teaching a concept. 
    
    Args: 
        concept: topic like 'SQL' or 'Python Logic'. 
        level: 'Seed'|'Sapling'|'Tree'.
        
    Returns:
        JSON string representing the game.
    """
    prompt = f"""Build a {level} mini-game teaching '{concept}'. Output ONLY the JSON shape:

{{
  "title": "Tenali's Array Sort",
  "concept_taught": "{concept}",
  "level": "{level}",
  "steps": [
    {{"step_type":"dialogue","paati_says":"Vaa kanna, innikku naama {concept} padikkalaam!","visual":"🥭","points_on_success":0}},
    {{"step_type":"choice","paati_says":"Enna aachu?","visual":"🤔","question":"Which SQL JOIN returns all rows from left table?","options":["INNER JOIN","LEFT JOIN","RIGHT JOIN"],"correct_answer":"LEFT JOIN","points_on_success":20}},
    {{"step_type":"sequence","paati_says":"Sari varisaiyil vayyu!","visual":"📋","question":"Put the SQL clauses in order:","sequence_items":["WHERE","SELECT","FROM"],"correct_sequence":["SELECT","FROM","WHERE"],"correct_answer":"SELECT, FROM, WHERE","points_on_success":20}},
    {{"step_type":"dialogue","paati_says":"Romba nalla pannite kanna!","visual":"🏆","points_on_success":0}}
  ],
  "final_reward_badge": "Logic Master"
}}

Now output the real game JSON for '{concept}' at level '{level}'. Every step MUST have a 'visual' emoji field."""

    for attempt in range(2):
        suffix = "" if attempt == 0 else " Be extra careful: include ALL required fields and follow the schema exactly."
        game = game_agent.run(prompt + suffix)
        content = game.content
        if isinstance(content, MiniGame):
            return content.model_dump_json()
        try:
            data = content if isinstance(content, str) else json.dumps(content)
            return MiniGame.model_validate_json(data).model_dump_json()
        except Exception:
            continue
    return json.dumps({"error": "schema_parse_failed"})

@tool(name="award_paati_points", description="Award Paati-Kural points to the student for a correct answer or completed challenge.")
def award_paati_points(points: int, reason: str) -> str:
    """
    Args:
        points: Number of points to award (e.g., 10, 50, 100).
        reason: Short reason for the award (e.g., "Solved Tenali math puzzle").
    Returns:
        Confirmation string.
    """
    return f"Successfully awarded {points} points to student for {reason}."

@tool(name="check_government_schemes", description="Look up real Tamil Nadu / Central government schemes relevant to the student.")
def check_government_schemes(cgpa: float, stream: str, skills: list[str]) -> str:
    """
    Look up real Tamil Nadu / Central government schemes relevant to the student.

    Args:
        cgpa: Student's CGPA.
        stream: Student's stream of study.
        skills: List of student's skills.

    Returns:
        A string summarizing relevant schemes.
    """
    schemes = [
        {"name": "Naan Mudhalvan", "link": "https://www.naanmudhalvan.tn.gov.in/", "criteria": "Engineering/Arts Students"},
        {"name": "TNSDC Skill Training", "link": "https://www.tnsdc.tn.gov.in/", "criteria": "Vocational Skills"},
        {"name": "PMKVY", "link": "https://www.pmkvyofficial.org/", "criteria": "General Skill Development"}
    ]
    relevant = [s for s in schemes if stream.lower() in s['criteria'].lower() or "General" in s['criteria']]
    return json.dumps(relevant)

@tool(name="get_tenali_puzzle", description="Retrieve a folklore-themed Samacheer logic puzzle for the given topic.")
def get_tenali_puzzle(topic: str) -> str:
    """
    Get a Tenali Raman style puzzle based on topic.

    Args:
        topic: The topic of the puzzle ("Math", "Logic").

    Returns:
        A puzzle string.
    """
    puzzles = {
        "Math": "Tenali Raman has 5 mangoes. He gives 2 to the King but the King doubles what is left. How many mangoes now?",
        "Logic": "If a cat in Madurai has 3 kittens, and each kitten has 2 spots, how many spots in total in the village?"
    }
    return puzzles.get(topic, "Tell a story about Thirukkural 1 (Agaram Muthala).")

@tool(name="safe_web_search", description="Search the web for real-time info.")
def safe_web_search(query: str) -> str:
    try:
        from ddgs import DDGS
        results = DDGS().text(query, max_results=3)
        res_list = list(results)
        if not res_list:
            return "No results found."
        return json.dumps(res_list)
    except Exception as e:
        return f"Search failed: {e}. Try a broader query."
