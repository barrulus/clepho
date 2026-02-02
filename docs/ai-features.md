# AI Features

Clepho integrates with local and cloud LLMs to provide AI-powered photo descriptions and semantic search capabilities.

## Overview

AI features in Clepho:

1. **Photo Descriptions** - Generate natural language descriptions of photos
2. **Semantic Search** - Find photos by describing what you're looking for
3. **Batch Processing** - Process entire directories automatically

## LLM Setup

### Supported Providers

| Provider | Type | Cost | Setup |
|----------|------|------|-------|
| LM Studio | Local | Free | Download app + model |
| Ollama | Local | Free | Install + pull model |
| OpenAI | Cloud | Paid | API key required |
| Anthropic | Cloud | Paid | API key required |

### LM Studio Setup

1. Download [LM Studio](https://lmstudio.ai/)
2. Download a vision-capable model:
   - Search for "llava" or "vision" models
   - Recommended: `llava-1.5-7b` or `bakllava`
3. Start the local server:
   - Click "Local Server" tab
   - Click "Start Server"
   - Default: `http://127.0.0.1:1234`

4. Configure Clepho:
   ```toml
   [llm]
   provider = "lmstudio"
   endpoint = "http://127.0.0.1:1234/v1"
   model = "llava-1.5-7b"
   ```

### Ollama Setup

1. Install [Ollama](https://ollama.ai/)
   ```bash
   curl -fsSL https://ollama.ai/install.sh | sh
   ```

2. Pull a vision model:
   ```bash
   ollama pull llava
   # or for better quality:
   ollama pull llava:13b
   ```

3. Start Ollama (usually automatic):
   ```bash
   ollama serve
   ```

4. Configure Clepho:
   ```toml
   [llm]
   provider = "ollama"
   endpoint = "http://127.0.0.1:11434"
   model = "llava"
   ```

### OpenAI Setup

1. Get API key from [OpenAI Platform](https://platform.openai.com/)

2. Configure Clepho:
   ```toml
   [llm]
   provider = "openai"
   endpoint = "https://api.openai.com/v1"
   model = "gpt-4-vision-preview"
   api_key = "sk-..."
   ```

### Anthropic Setup

1. Get API key from [Anthropic Console](https://console.anthropic.com/)

2. Configure Clepho:
   ```toml
   [llm]
   provider = "anthropic"
   endpoint = "https://api.anthropic.com"
   model = "claude-3-sonnet-20240229"
   api_key = "sk-ant-..."
   ```

## Generating Descriptions

### Single Photo

1. Navigate to a photo
2. Press `i` to describe with AI
3. Wait for processing
4. Description appears in preview pane

```
Generating description for photo_001.jpg...
Done: "A golden retriever playing fetch on a sandy beach..."
```

### Batch Processing

Process all photos in current directory:

1. Press `I` (uppercase) for batch processing
2. Progress shown in status bar: `[B:45%]`
3. View task details with `T`

```
Batch processing 150 photos...
[████████████████░░░░░░░░░░░░░░░░░░░░░░░] 45% (68/150)
```

### Scheduled Processing

Schedule batch processing for later:

1. Press `@` to open schedule dialog
2. Select "LLM Batch Process"
3. Set date/time
4. Press Enter

## Description Content

### What's Analyzed

The LLM examines:
- **Scene content** - Objects, people, animals, landscape
- **Actions** - What's happening in the photo
- **Setting** - Indoor/outdoor, time of day, weather
- **Mood** - Emotional tone of the image
- **Technical aspects** - Composition, lighting

### Example Descriptions

```
"A family of four having a picnic in a sunny park. The parents
are seated on a red checkered blanket while two children play
with a frisbee nearby. Large oak trees provide dappled shade,
and a lake is visible in the background."
```

```
"Close-up macro photograph of a honeybee collecting pollen from
a bright purple lavender flower. The bee's fuzzy body and
translucent wings are in sharp focus against a soft, blurred
green background."
```

## Semantic Search

Search your photo collection using natural language.

### Starting a Search

1. Press `/` to open search dialog
2. Type your search query
3. Press Enter to search
4. Navigate results with `j`/`k`
5. Press Enter to go to selected photo

### Search Examples

| Query | Finds |
|-------|-------|
| "beach sunset" | Photos described as beach sunsets |
| "dog playing" | Photos with dogs in action |
| "birthday party" | Celebration photos |
| "mountain landscape" | Scenic mountain photos |
| "people laughing" | Candid happy moments |

### Search Dialog

```
┌─────────────────────────────────────────────────────────────┐
│ Search: birthday cake                                       │
├─────────────────────────────────────────────────────────────┤
│ Results (12 matches):                                       │
│                                                             │
│ > [92%] IMG_4521.jpg - Birthday celebration with...        │
│   [87%] party_2024.jpg - Children gathered around...       │
│   [85%] celebration.jpg - A decorated chocolate...         │
│   [71%] family_dinner.jpg - Family seated at table...      │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ j/k:navigate Enter:go to photo Esc:close                   │
└─────────────────────────────────────────────────────────────┘
```

### Search Scoring

Results are ranked by relevance:
- **90-100%** - Strong match
- **70-90%** - Good match
- **50-70%** - Partial match
- **Below 50%** - Weak match (usually filtered)

## Embeddings

For semantic search, Clepho uses text embeddings:

### How It Works

1. Photo descriptions are converted to vectors (embeddings)
2. Search queries are converted to vectors
3. Cosine similarity finds closest matches

### Embedding Storage

Embeddings are stored in the database:
- ~1.5KB per photo
- Enables instant search
- No LLM needed for searching

### Without Embeddings

If embeddings aren't configured, search falls back to:
- Text matching on descriptions
- Less accurate but still useful

## Processing Status

### Status Indicators

| Indicator | Meaning |
|-----------|---------|
| `[L:...]` | Single LLM description in progress |
| `[B:45%]` | Batch processing at 45% |
| No indicator | No AI processing active |

### Task List

Press `T` to view detailed task status:

```
┌─────────────────────────────────────────────────────────────┐
│ Running Tasks                                               │
├─────────────────────────────────────────────────────────────┤
│ > LLM Batch Process - 45% (68/150) - vacation/photo_068.jpg│
│   Elapsed: 5:32                                            │
├─────────────────────────────────────────────────────────────┤
│ c:cancel task  Esc:close                                   │
└─────────────────────────────────────────────────────────────┘
```

## Performance

### Processing Speed

| Provider | Speed | Quality |
|----------|-------|---------|
| LM Studio (7B) | ~5 sec/photo | Good |
| LM Studio (13B) | ~15 sec/photo | Better |
| Ollama (llava) | ~3 sec/photo | Good |
| OpenAI GPT-4V | ~2 sec/photo | Excellent |

### Resource Usage

**Local LLMs:**
- GPU: 4-8GB VRAM recommended
- CPU: Slower but works
- RAM: 8-16GB for 7B models

**Cloud APIs:**
- Minimal local resources
- Network bandwidth for images
- API costs per image

## Best Practices

### Model Selection

| Use Case | Recommended Model |
|----------|-------------------|
| Quick processing | llava (7B) |
| Better descriptions | llava:13b |
| Best quality | GPT-4 Vision |
| Privacy-focused | Local models only |

### Batch Processing Tips

1. **Start small** - Test with a few photos first
2. **Schedule overnight** - Large batches take time
3. **Check quality** - Verify descriptions are useful
4. **Re-process selectively** - Only re-do poor descriptions

### Search Optimization

1. **Be specific** - "red car on highway" > "car"
2. **Use natural language** - Write like you'd describe it
3. **Try variations** - "dog", "puppy", "canine"
4. **Combine concepts** - "sunset beach palm trees"

## Troubleshooting

### LLM Connection Failed

```
Error: Failed to connect to LLM endpoint
```

**Solutions:**
- Verify LM Studio/Ollama is running
- Check endpoint URL in config
- Check firewall settings
- Test with: `curl http://127.0.0.1:1234/v1/models`

### Slow Processing

**Local models:**
- Use GPU if available
- Try smaller model
- Close other GPU applications

**Cloud APIs:**
- Check internet connection
- Verify API quota
- Try during off-peak hours

### Poor Descriptions

**Solutions:**
- Try a larger/better model
- Ensure image isn't corrupted
- Check image isn't too small
- Cloud APIs generally more accurate

### Search Not Finding Photos

- Ensure photos have descriptions (`i` first)
- Check search terms match description style
- Try broader search terms
- Verify embeddings were generated
