import torch
from refiners.foundationals.latent_diffusion import StableDiffusion_1

a = 1 + 2

assert a == 1 + 3

sd = StableDiffusion_1()
x = torch.randn(5, 3)
  
y = StableDiffusion_1(x)